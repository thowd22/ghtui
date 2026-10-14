//! Strict offline values for the undeclared metadata action slice. Raw ids and
//! distinct PR/Issue namespaces are fixture facts, not harvested snapshots.
use super::{model, Fixture, GhStep};
use model::{
    pr_metadata_actions::PrMetadataCommand,
    providers::{
        gh::{
            GithubProvider, HostIdentity, PrContext, PrMetadataCatalogueRead, PrMetadataReadLimits,
            PrSummary,
        },
        runner::CancellationToken,
    },
};
use serde_json::{json, Value};
use std::{fs, sync::Arc};
pub(super) fn identity() -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "requester".into(),
        account_id: Some(42),
        gh_version: "gh version 2.101.0".into(),
    }
}
pub(super) fn repository() -> Value {
    json!({"id":410,"node_id":"R_410","full_name":"example/repo","description":"Private","default_branch":"main","private":true,"archived":false,"permissions":{"pull":true,"push":true,"maintain":false,"admin":false}})
}
pub(super) fn user(id: u64, name: &str) -> Value {
    json!({"id":id,"node_id":format!("U_{id}"),"login":name,"type":"User","permissions":{"pull":true,"push":false}})
}
pub(super) fn label(id: u64, name: &str) -> Value {
    json!({"id":id,"node_id":format!("L_{id}"),"name":name})
}
pub(super) fn team(id: u64, name: &str) -> Value {
    json!({"id":id,"node_id":format!("T_{id}"),"slug":name,"name":name,"permission":"pull","organization":{"login":"example","id":310,"node_id":"O_310"}})
}
pub(super) fn milestone(id: u64, number: u64) -> Value {
    json!({"id":id,"node_id":format!("M_{id}"),"number":number,"title":format!("Milestone {number}"),"state":"open"})
}
pub(super) fn parent() -> Value {
    json!({"id":1007,"node_id":"PR_7","number":7,"title":"Unchanged title","body":"Private description must never be in a receipt","state":"open","draft":false,"merged":false,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-13T00:00:00Z","user":user(51,"author"),"head":{"sha":"a".repeat(40),"ref":"feature","repo":repository()},"base":{"sha":"b".repeat(40),"ref":"main","repo":repository()},"html_url":"https://github.invalid/example/repo/pull/7","requested_reviewers":[user(70,"current-reviewer")],"requested_teams":[team(75,"current-team")],"labels":[label(80,"existing ☃")],"assignees":[user(60,"current-assignee")],"milestone":milestone(85,5),"maintainer_can_modify":false,"mergeable":true,"mergeable_state":"clean","additions":0,"deletions":0,"changed_files":0})
}
pub(super) fn issue_from(parent: &Value) -> Value {
    let mut issue = parent.clone();
    issue["id"] = json!(2007);
    issue["node_id"] = json!("I_2007");
    issue["pull_request"] = json!({"url":"https://github.invalid/api/v3/repos/example/repo/pulls/7","html_url":"https://github.invalid/example/repo/pull/7"});
    issue
}
pub(super) fn original() -> (PrSummary, PrContext) {
    let summary:PrSummary=serde_json::from_value(json!({"pr":{"host":"github.invalid","id":"PR_7","repository":"example/repo","number":7,"title":"Unchanged title","body":"Private description must never be in a receipt","url":"https://github.invalid/example/repo/pull/7","author":{"login":"author","actor_type":"User"},"state":"open","draft":false,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-13T00:00:00Z","queues":[]},"head_sha":"a".repeat(40),"base_sha":"b".repeat(40),"head_ref":"feature","base_ref":"main","merged":false,"auto_merge":false,"merge_commit_sha":null,"mergeable":true,"mergeable_state":"clean","additions":0,"deletions":0,"changed_files":0})).unwrap();
    let repo = json!({"host":"github.invalid","id":410,"full_name":"example/repo","description":"Private","default_branch":"main","private":true,"archived":false,"permissions":{"pull":true,"push":true,"maintain":false,"admin":false}});
    let context:PrContext=serde_json::from_value(json!({"repository":repo,"base_repository":repo,"head_repository":repo,"is_cross_repository":false,"maintainer_can_modify":false,"head_sha":"a".repeat(40),"base_sha":"b".repeat(40),"issue_metadata":issue_from(&parent()),"requested_reviewers":[],"requested_teams":[],"source_urls":[]})).unwrap();
    (summary, context)
}
pub(super) fn provider(f: &Fixture) -> GithubProvider {
    GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(f.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", f.root.join("gh-fixture"))
}
pub(super) fn limits() -> PrMetadataReadLimits {
    PrMetadataReadLimits {
        page_size: 2,
        max_rows: 10,
        max_pages: 3,
        ..Default::default()
    }
}
pub(super) fn get(endpoint: &str, value: Value) -> GhStep {
    GhStep {
        args: [
            "api",
            endpoint,
            "--hostname",
            "github.invalid",
            "--method",
            "GET",
        ]
        .map(str::to_owned)
        .to_vec(),
        stdin: vec![],
        stdout: serde_json::to_vec(&value).unwrap(),
        stderr: vec![],
        status: 0,
        delay_ms: 0,
    }
}
pub(super) fn graphql(query: &str, variables: Value, value: Value) -> GhStep {
    GhStep {
        args: [
            "api",
            "graphql",
            "--hostname",
            "github.invalid",
            "--method",
            "POST",
            "--input",
            "-",
        ]
        .map(str::to_owned)
        .to_vec(),
        stdin: serde_json::to_vec(&json!({"query":query,"variables":variables})).unwrap(),
        stdout: serde_json::to_vec(&value).unwrap(),
        stderr: vec![],
        status: 0,
        delay_ms: 0,
    }
}
pub(super) fn candidate(command: PrMetadataCommand) -> Value {
    match command {
        PrMetadataCommand::AddLabel | PrMetadataCommand::RemoveLabel => label(81, "new 🐛"),
        PrMetadataCommand::AddAssignee | PrMetadataCommand::RemoveAssignee => {
            user(61, "new-assignee")
        }
        PrMetadataCommand::AddReviewer | PrMetadataCommand::RemoveReviewer => {
            user(71, "new-reviewer")
        }
        PrMetadataCommand::AddTeam | PrMetadataCommand::RemoveTeam => team(76, "new-team"),
        _ => milestone(86, 6),
    }
}
pub(super) fn field(command: PrMetadataCommand) -> &'static str {
    match command {
        PrMetadataCommand::AddLabel | PrMetadataCommand::RemoveLabel => "labels",
        PrMetadataCommand::AddAssignee | PrMetadataCommand::RemoveAssignee => "assignees",
        PrMetadataCommand::AddReviewer | PrMetadataCommand::RemoveReviewer => "reviewers",
        PrMetadataCommand::AddTeam | PrMetadataCommand::RemoveTeam => "teams",
        _ => "milestone",
    }
}
pub(super) fn changed(command: PrMetadataCommand) -> Value {
    let mut value = parent();
    let next = candidate(command);
    let key = match field(command) {
        "reviewers" => "requested_reviewers",
        "teams" => "requested_teams",
        other => other,
    };
    match command {
        PrMetadataCommand::SetMilestone => value[key] = next,
        PrMetadataCommand::ClearMilestone => value[key] = Value::Null,
        PrMetadataCommand::AddLabel
        | PrMetadataCommand::AddAssignee
        | PrMetadataCommand::AddReviewer
        | PrMetadataCommand::AddTeam => value[key].as_array_mut().unwrap().push(next),
        _ => value[key] = json!([]),
    }
    value
}
pub(super) fn members(command: PrMetadataCommand, parent: &Value) -> Value {
    let mut node = json!({"__typename":"PullRequest","id":"PR_7","fullDatabaseId":"1007","number":7,"headRefOid":"a".repeat(40),"baseRefOid":"b".repeat(40),"headRefName":"feature","baseRefName":"main","state":"OPEN","isDraft":false,"repository":{"id":"R_410","nameWithOwner":"example/repo"}});
    let member = |raw: &Value, kind: &str| {
        let mut result = json!({"id":raw["node_id"]});
        match kind {
            "labels" => result["name"] = raw["name"].clone(),
            "milestone" => {
                result["number"] = raw["number"].clone();
                result["title"] = raw["title"].clone();
            }
            "teams" => {
                result["slug"] = raw["slug"].clone();
                result["organization"] = json!({"login":"example"});
            }
            _ => result["login"] = raw["login"].clone(),
        };
        result
    };
    let kind = field(command);
    if kind == "milestone" {
        node["milestone"] = if parent["milestone"].is_null() {
            Value::Null
        } else {
            member(&parent["milestone"], kind)
        };
    } else if matches!(kind, "reviewers" | "teams") {
        let mut rows = vec![];
        for (key, typename, kind) in [
            ("requested_reviewers", "User", "reviewers"),
            ("requested_teams", "Team", "teams"),
        ] {
            for raw in parent[key].as_array().unwrap() {
                let mut who = member(raw, kind);
                who["__typename"] = json!(typename);
                rows.push(json!({"id":format!("RR_{}",raw["id"]),"requestedReviewer":who}));
            }
        }
        node["reviewRequests"] =
            json!({"totalCount":rows.len(),"pageInfo":{"hasNextPage":false},"nodes":rows});
    } else {
        let rows = parent[kind]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| member(r, kind))
            .collect::<Vec<_>>();
        node[kind] = json!({"totalCount":rows.len(),"pageInfo":{"hasNextPage":false},"nodes":rows});
    }
    let mut result = json!({"data":{"node":node}});
    if command == PrMetadataCommand::AddTeam {
        result["data"]["candidate"] = json!({"__typename":"Team","id":"T_76","slug":"new-team","organization":{"login":"example"}});
    }
    result
}
pub(super) fn member_variables(command: PrMetadataCommand) -> Value {
    let mut variables = json!({"id":"PR_7"});
    if field(command) == "teams" {
        variables["candidate"] = json!(if command == PrMetadataCommand::AddTeam {
            "T_76"
        } else {
            "T_75"
        });
        variables["includeCandidate"] = json!(command == PrMetadataCommand::AddTeam);
    }
    variables
}
pub(super) fn preparation(command: PrMetadataCommand) -> Vec<GhStep> {
    preparation_from(command, &parent())
}
pub(super) fn preparation_from(command: PrMetadataCommand, original: &Value) -> Vec<GhStep> {
    let p = original.clone();
    let mut calls = catalogue(command, &p, vec![candidate(command)]);
    calls.push(graphql(
        &model::providers::gh::pr_metadata_members_query(field(command)).unwrap(),
        member_variables(command),
        members(command, &p),
    ));
    calls.extend([
        get("repos/example/repo", repository()),
        get("repos/example/repo/pulls/7", p),
        get("user", json!({"id":42,"login":"requester"})),
    ]);
    calls
}
pub(super) fn mutation_input(command: PrMetadataCommand) -> Value {
    match command {
        PrMetadataCommand::AddLabel => json!({"pullRequestId":"PR_7","labelIds":["L_80","L_81"]}),
        PrMetadataCommand::RemoveLabel => json!({"pullRequestId":"PR_7","labelIds":[]}),
        PrMetadataCommand::AddAssignee => {
            json!({"pullRequestId":"PR_7","assigneeIds":["U_60","U_61"]})
        }
        PrMetadataCommand::RemoveAssignee => json!({"pullRequestId":"PR_7","assigneeIds":[]}),
        PrMetadataCommand::AddReviewer => {
            json!({"pullRequestId":"PR_7","union":true,"userIds":["U_71"]})
        }
        PrMetadataCommand::AddTeam => {
            json!({"pullRequestId":"PR_7","union":true,"teamIds":["T_76"]})
        }
        PrMetadataCommand::RemoveReviewer => {
            json!({"pullRequestId":"PR_7","union":false,"userIds":[],"teamIds":["T_75"]})
        }
        PrMetadataCommand::RemoveTeam => {
            json!({"pullRequestId":"PR_7","union":false,"userIds":["U_70"],"teamIds":[]})
        }
        PrMetadataCommand::SetMilestone => json!({"pullRequestId":"PR_7","milestoneId":"M_86"}),
        PrMetadataCommand::ClearMilestone => json!({"pullRequestId":"PR_7","milestoneId":null}),
    }
}
pub(super) fn publication(command: PrMetadataCommand) -> Vec<GhStep> {
    publication_from(command, &changed(command), mutation_input(command))
}
pub(super) fn publication_from(
    command: PrMetadataCommand,
    final_parent: &Value,
    input: Value,
) -> Vec<GhStep> {
    let reviews = matches!(
        command,
        PrMetadataCommand::AddReviewer
            | PrMetadataCommand::AddTeam
            | PrMetadataCommand::RemoveReviewer
            | PrMetadataCommand::RemoveTeam
    );
    let mutation = if reviews {
        model::providers::gh::PR_METADATA_REQUEST_REVIEWS_MUTATION
    } else {
        model::providers::gh::PR_METADATA_UPDATE_MUTATION
    };
    let p = final_parent.clone();
    vec![
        graphql(
            mutation,
            json!({"input":input}),
            if reviews {
                json!({"data":{"requestReviews":{"pullRequest":{"id":"PR_7"}}}})
            } else {
                json!({"data":{"updatePullRequest":{"pullRequest":{"id":"PR_7"}}}})
            },
        ),
        graphql(
            &model::providers::gh::pr_metadata_members_query(field(command)).unwrap(),
            member_variables(command),
            members(command, &p),
        ),
        get("repos/example/repo", repository()),
        get("repos/example/repo/pulls/7", p.clone()),
        get("repos/example/repo/issues/7", issue_from(&p)),
        get("user", json!({"id":42,"login":"requester"})),
    ]
}
pub(super) fn catalogue(
    command: PrMetadataCommand,
    parent: &Value,
    rows: Vec<Value>,
) -> Vec<GhStep> {
    let path = match command {
        PrMetadataCommand::AddLabel | PrMetadataCommand::RemoveLabel => "labels",
        PrMetadataCommand::AddAssignee | PrMetadataCommand::RemoveAssignee => "assignees",
        PrMetadataCommand::AddReviewer | PrMetadataCommand::RemoveReviewer => "collaborators",
        PrMetadataCommand::AddTeam | PrMetadataCommand::RemoveTeam => "teams",
        _ => "milestones?state=open",
    };
    vec![
        get("user", json!({"id":42,"login":"requester"})),
        get("repos/example/repo", repository()),
        get("repos/example/repo/pulls/7", parent.clone()),
        get("repos/example/repo/issues/7", issue_from(parent)),
        get(
            &format!(
                "repos/example/repo/{path}{}per_page=2&page=1",
                if path.contains('?') { '&' } else { '?' }
            ),
            json!(rows),
        ),
        get("repos/example/repo", repository()),
        get("repos/example/repo/pulls/7", parent.clone()),
        get("user", json!({"id":42,"login":"requester"})),
    ]
}
pub(super) fn load(f: &Fixture, command: PrMetadataCommand) -> Arc<PrMetadataCatalogueRead> {
    let (target, context) = original();
    Arc::new(
        provider(f)
            .metadata_catalogue_read(
                &identity(),
                &target,
                &context,
                command.catalogue(),
                &limits(),
                &CancellationToken::default(),
            )
            .unwrap(),
    )
}
pub(super) fn journal(f: &Fixture) -> Vec<Value> {
    fs::read_to_string(f.root.join("gh-fixture/commands.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}
pub(super) fn assert_commands(f: &Fixture, count: usize, writes: usize) {
    let rows = journal(f);
    assert_eq!(rows.len(), count);
    assert!(rows.iter().all(|r| r["matched"] == true));
    let observed = rows
        .iter()
        .filter(|r| {
            r["stdin_hex"].as_str().is_some_and(|s| {
                s.starts_with("7b")
                    && String::from_utf8(hex_decode(s))
                        .unwrap()
                        .contains("mutation(")
            })
        })
        .count();
    assert_eq!(observed, writes);
}
fn hex_decode(raw: &str) -> Vec<u8> {
    raw.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
