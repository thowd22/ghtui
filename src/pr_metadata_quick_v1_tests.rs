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
