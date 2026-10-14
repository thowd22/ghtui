//! Undeclared child of metadata_actions: complete mixed aggregate preservation.
use super::*;
#[test]
fn pr_metadata_actions_provider_mixed_aggregate_removal_preserves_remaining_user_team_ids_and_issue_fields(
) {
    for command in [
        PrMetadataCommand::RemoveReviewer,
        PrMetadataCommand::RemoveTeam,
    ] {
        let f = Fixture::new();
        let mut first = parent();
        first["requested_reviewers"]
            .as_array_mut()
            .unwrap()
            .push(user(72, "another-reviewer"));
        first["requested_teams"]
            .as_array_mut()
            .unwrap()
            .push(team(77, "another-team"));
        let mut last = first.clone();
        last[if command == PrMetadataCommand::RemoveReviewer {
            "requested_reviewers"
        } else {
            "requested_teams"
        }]
        .as_array_mut()
        .unwrap()
        .remove(0);
        let input = if command == PrMetadataCommand::RemoveReviewer {
            json!({"pullRequestId":"PR_7","union":false,"userIds":["U_72"],"teamIds":["T_75","T_77"]})
        } else {
            json!({"pullRequestId":"PR_7","union":false,"userIds":["U_70","U_72"],"teamIds":["T_77"]})
        };
        let mut calls = catalogue(command, &first, vec![candidate(command)]);
        calls.extend(preparation_from(command, &first));
        calls.extend(publication_from(command, &last, input.clone()));
        f.scripted_gh(&calls);
        let plan = load_plan(&f, command);
        assert!(plan.command().aggregate_review_replacement());
        assert_eq!(plan.after().len(), 1);
        assert_eq!(plan.preserved_review_members().len(), 2);
        let result = execute(&f, &plan);
        let PrMetadataExecution::Confirmed { receipt, .. } = result else {
            panic!("expected exact mixed aggregate confirmation")
        };
        assert_eq!(
            receipt.preserved_review_ids(),
            if command == PrMetadataCommand::RemoveReviewer {
                &[75, 77][..]
            } else {
                &[70, 72][..]
            }
        );
        for field in ["labels", "assignees", "milestone", "title", "body"] {
            assert_eq!(first[field], last[field]);
            assert!(input.get(field).is_none());
        }
        assert_commands(&f, 26, 1);
    }
}
#[test]
fn pr_metadata_actions_provider_unknown_rest_team_namespace_needs_genuine_current_and_candidate_node_proof(
) {
    for wrong in [false, true] {
        let command = PrMetadataCommand::AddTeam;
        let f = Fixture::new();
        let mut first = parent();
        first["requested_teams"][0]
            .as_object_mut()
            .unwrap()
            .remove("organization");
        let mut selected = candidate(command);
        selected.as_object_mut().unwrap().remove("organization");
        let mut last = first.clone();
        last["requested_teams"]
            .as_array_mut()
            .unwrap()
            .push(selected.clone());
        let mut calls = catalogue(command, &first, vec![selected.clone()]);
        let mut prepare = preparation_from(command, &first);
        prepare[4].stdout = serde_json::to_vec(&json!([selected])).unwrap();
        if wrong {
            let mut graph = members(command, &first);
            graph["data"]["candidate"]["organization"]["login"] = json!("other-owner");
            prepare[8].stdout = serde_json::to_vec(&graph).unwrap();
            prepare.truncate(9);
        }
        calls.extend(prepare);
        if !wrong {
            calls.extend(publication_from(command, &last, mutation_input(command)));
        }
        let count = calls.len();
        f.scripted_gh(&calls);
        let plan = load_plan(&f, command);
        assert_eq!(plan.selected().unwrap().namespace(), None);
        assert_eq!(plan.before()[0].namespace(), None);
        let result = execute(&f, &plan);
        if wrong {
            assert!(matches!(result, PrMetadataExecution::RefusedPrivate { .. }));
            assert_commands(&f, count, 0);
        } else {
            assert!(matches!(result, PrMetadataExecution::Confirmed { .. }));
            assert_commands(&f, 26, 1);
        }
    }
}
#[test]
fn pr_metadata_actions_provider_aggregate_both_sets_preclaim_concurrency_and_postwrite_reused_node_are_not_hidden(
) {
    let command = PrMetadataCommand::RemoveReviewer;
    for after in [false, true] {
        let f = Fixture::new();
        let mut calls = whole(command);
        if after {
            let mut graph = members(command, &changed(command));
            graph["data"]["node"]["reviewRequests"]["nodes"][0]["requestedReviewer"]["id"] =
                json!("T_reused_slug");
            calls[21].stdout = serde_json::to_vec(&graph).unwrap();
            calls.truncate(22);
        } else {
            let mut fresh = parent();
            fresh["requested_teams"][0]["id"] = json!(999);
            fresh["requested_teams"][0]["node_id"] = json!("T_999");
            let replacement = catalogue(command, &fresh, vec![candidate(command)]);
            calls.splice(8..16, replacement);
            calls.truncate(16);
        }
        let count = calls.len();
        f.scripted_gh(&calls);
        let plan = load_plan(&f, command);
        let result = execute(&f, &plan);
        if after {
            assert!(matches!(result, PrMetadataExecution::Uncertain { .. }));
            assert_commands(&f, count, 1);
        } else {
            assert!(matches!(result, PrMetadataExecution::Refused { .. }));
            assert_commands(&f, count, 0);
        }
    }
}
