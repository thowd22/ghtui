//! Strict native metadata values adapted from
//! tests/support/metadata_actions.rs. Backend-only probe helpers intentionally
//! remain in their original module; no lint suppression or default acceptance.
use super::{GhStep, PrMetadataCommand};
use serde_json::{json, Value};

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
    json!({"id":1007,"node_id":"PR_7","number":7,"title":"Unchanged title","body":"Private description must never be in a receipt","state":"open","draft":false,"merged":false,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-13T00:00:00Z","user":user(42,"requester"),"head":{"sha":"a".repeat(40),"ref":"feature","repo":repository()},"base":{"sha":"b".repeat(40),"ref":"main","repo":repository()},"html_url":"https://github.invalid/example/repo/pull/7","requested_reviewers":[user(70,"current-reviewer")],"requested_teams":[team(75,"current-team")],"labels":[label(80,"existing ☃")],"assignees":[user(60,"current-assignee")],"milestone":milestone(85,5),"maintainer_can_modify":false,"mergeable":true,"mergeable_state":"clean","additions":0,"deletions":0,"changed_files":0})
}

pub(super) fn issue_from(parent: &Value) -> Value {
    let mut issue = parent.clone();
    issue["id"] = json!(2007);
    issue["node_id"] = json!("I_2007");
    issue["pull_request"] = json!({"url":"https://github.invalid/api/v3/repos/example/repo/pulls/7","html_url":"https://github.invalid/example/repo/pull/7"});
    issue
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
