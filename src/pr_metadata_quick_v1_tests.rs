use super::*;
use crate::github_workspace::{metadata_access_v1, metadata_rows_v51};
use serde_json::json;

fn context(issue: serde_json::Value, push: bool) -> crate::providers::gh::PrContext {
    let repo = json!({"host":"github.com","id":1,"full_name":"o/r","description":"",
        "default_branch":"main","private":false,"archived":false,
        "permissions":{"pull":true,"push":push,"maintain":false,"admin":false}});
    serde_json::from_value(json!({
        "repository": repo, "issue_metadata": issue, "requested_reviewers": [],
        "requested_teams": [], "head_repository": null, "base_repository": repo,
        "is_cross_repository": false, "maintainer_can_modify": false,
        "head_sha": "a", "base_sha": "b", "source_urls": []
    }))
    .unwrap()
}
fn text(context: &crate::providers::gh::PrContext) -> String {
    metadata_rows_v51(context, None)
        .iter()
        .map(|row| row.label.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn v1_pr_metadata_quick_keys_are_unique_cover_every_command_and_avoid_tab_keys() {
    let keys: std::collections::BTreeSet<_> = CONTROLS.iter().map(|(_, k, _, _)| *k).collect();
    assert_eq!(keys.len(), CONTROLS.len());
    for command in PrMetadataCommand::ALL {
        assert!(
            CONTROLS.iter().any(|(c, _, _, _)| *c == command),
            "{command:?}"
        );
    }
    for (_, _, tab_key) in PrTab::ALL {
        assert!(!keys.contains(&tab_key), "collides with tab key {tab_key}");
    }
    for reserved in ['p', 'b', 'r', 'q', 's', 'x', '?', ':', 'J', 'T', 'K'] {
        assert!(!keys.contains(&reserved), "collides with {reserved}");
    }
    assert_eq!(priority(PrMetadataCommand::AddLabel), 20);
}

#[test]
fn v1_pr_metadata_access_summary_is_plain_language() {
    assert!(
        metadata_access_v1([Some(true), Some(true), Some(false), Some(false)])
            .starts_with("You can change labels")
    );
    assert!(
        metadata_access_v1([Some(true), Some(false), Some(false), Some(false)])
            .starts_with("View only")
    );
    assert!(metadata_access_v1([Some(true), None, None, None]).starts_with("Edit access unknown"));
}

#[test]
fn v1_pr_metadata_rows_say_none_for_empty_sets_and_list_present_values() {
    let empty = text(&context(
        json!({"labels":[],"assignees":[],"milestone":null,"locked":false}),
        true,
    ));
    for line in [
        "Labels: none",
        "Assignees: none",
        "Requested reviewers: none",
        "Milestone: none",
        "You can change labels",
    ] {
        assert!(empty.contains(line), "{line} missing from\n{empty}");
    }
    let full = text(&context(
        json!({"labels":[{"name":"bug"}],"assignees":[{"login":"octo","type":"User"}],
            "milestone":{"title":"v1"},"locked":false}),
        false,
    ));
    assert!(full.contains("Label: bug"));
    assert!(full.contains("Milestone: v1"));
    assert!(full.contains("View only"));
    assert!(!full.contains("Labels: none"));
    assert!(!full.contains("Assignees: none"));
    assert!(full.contains("Requested reviewers: none"));
}

#[test]
fn v1_pr_metadata_lists_issues_linked_by_closing_keywords() {
    use crate::github_workspace::closing_keyword_issues_v1;
    assert_eq!(
        closing_keyword_issues_v1(
            "Fixes #12. Also closes: other/repo#3, resolves #12 and mentions #99;\nfix #abc FIXED #7)"
        ),
        ["#12", "other/repo#3", "#7"]
    );
    let linked = text(&context(
        json!({"labels":[],"assignees":[],"milestone":null,"locked":false,"body":"Closes #41"}),
        true,
    ));
    assert!(
        linked.contains("Linked issues: #41 • closing keywords in Description; closed on merge to the default branch"),
        "{linked}"
    );
    let none = text(&context(
        json!({"labels":[],"assignees":[],"milestone":null,"locked":false,"body":"See #41"}),
        true,
    ));
    assert!(
        none.contains("Linked issues: none via closing keywords"),
        "{none}"
    );
}

fn repository(full_name: &str, permissions: serde_json::Value) -> serde_json::Value {
    json!({"host":"github.com","id":1,"full_name":full_name,"description":"",
        "default_branch":"main","private":false,"archived":false,"permissions":permissions})
}

#[test]
fn v13_pr_metadata_permissions_are_three_state_and_unknown_is_never_denied() {
    // The raw repository document omitted push and maintain: those flags are
    // unknown, not false, so edit access is unknown rather than view only.
    let mut unknown = context(
        json!({"labels":[],"assignees":[],"milestone":null,"locked":false}),
        false,
    );
    unknown.permission_flags_v13 = Some(crate::providers::gh::permission_flags_v13(&json!({
        "permissions":{"pull":true,"admin":false}
    })));
    let rows = text(&unknown);
    assert!(
        rows.contains(
            "Your repository permissions: read true • write unavailable • maintain unavailable • admin false"
        ),
        "{rows}"
    );
    assert!(rows.contains("Edit access unknown"), "{rows}");
    assert!(!rows.contains("View only"), "{rows}");
    // No permissions object at all: every flag is unknown.
    unknown.permission_flags_v13 =
        Some(crate::providers::gh::permission_flags_v13(&json!({"id":1})));
    assert!(text(&unknown).contains(
        "read unavailable • write unavailable • maintain unavailable • admin unavailable"
    ));
    // Reported flags are shown exactly, true and false alike.
    unknown.permission_flags_v13 = Some(crate::providers::gh::permission_flags_v13(&json!({
        "permissions":{"pull":true,"push":true,"maintain":false,"admin":false}
    })));
    let rows = text(&unknown);
    assert!(rows.contains("read true • write true • maintain false • admin false"));
    assert!(rows.contains("You can change labels"));
    // A context serialized before TASK-13 has no flags and keeps its booleans.
    let old: crate::providers::gh::PrContext =
        serde_json::from_value(serde_json::to_value(context(json!({}), true)).unwrap()).unwrap();
    assert_eq!(old.permission_flags_v13, None);
    assert!(text(&old).contains("read true • write true • maintain false • admin false"));
    assert!(!serde_json::to_string(&old)
        .unwrap()
        .contains("permission_flags_v13"));
}

#[test]
fn v13_pr_metadata_fork_origin_reviewers_teams_and_linked_issues_are_visible() {
    let issue = json!({"labels":[],"assignees":[],"milestone":null,"locked":false,
        "body":"Fixes #12 and closes org/beta#3"});
    let mut fork = context(issue.clone(), true);
    fork.base_repository = serde_json::from_value(repository("org/alpha", json!({}))).unwrap();
    fork.head_repository =
        Some(serde_json::from_value(repository("contributor/alpha", json!({}))).unwrap());
    fork.is_cross_repository = true;
    fork.requested_reviewers = vec![crate::providers::gh::Actor {
        login: "requested-reviewer".into(),
        actor_type: "User".into(),
        ..Default::default()
    }];
    fork.requested_teams = vec![crate::providers::gh::RequestedTeam {
        id: 10,
        slug: "security".into(),
        name: "Security team".into(),
    }];
    let rows = metadata_rows_v51(&fork, None);
    let labels: Vec<&str> = rows.iter().map(|row| row.label.as_str()).collect();
    assert!(
        labels.contains(&"Fork origin: contributor/alpha • fork of org/alpha"),
        "{labels:?}"
    );
    assert!(labels.contains(&"Review team: Security team (security) • @org/security"));
    assert!(labels.contains(
        &"Linked issues: #12, org/beta#3 • closing keywords in Description; closed on merge to the default branch"
    ));
    assert!(!labels.contains(&"Requested reviewers: none"));
    // Requested users are shared actor rows (chip + @login), never plain text.
    let reviewer = rows
        .iter()
        .find(|row| row.label == "review requested")
        .expect("requested reviewer row");
    assert_eq!(
        reviewer.actor.as_ref().map(|actor| actor.login.as_str()),
        Some("requested-reviewer")
    );
    // Same repository: no fork. Deleted head repository: said so.
    let mut same = context(issue.clone(), true);
    same.head_repository = Some(same.base_repository.clone());
    assert!(text(&same).contains("Fork origin: none • head branch is in o/r"));
    let gone = context(issue, true);
    assert!(text(&gone).contains("Fork origin: deleted or unavailable"));
}
