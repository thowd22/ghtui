//! UNREGISTERED strict source-only ID-native metadata writer contracts.
use super::{Fixture, GhStep};
use ghtui as model;
use model::{
    pr_metadata_actions::{
        PrMetadataCommand, PrMetadataExecution, PrMetadataExecutionContext, PrMetadataPlan,
        PrMetadataRefresh,
    },
    pr_write_journal::PrWriteJournal,
    providers::{
        gh::{GithubErrorKind, PrMetadataReadLimits},
        runner::CancellationToken,
    },
};
use serde_json::{json, Value};
use std::time::Duration;
#[path = "../support/metadata_actions.rs"]
mod fixture;
use fixture::*;
#[path = "metadata_actions_review_sets.rs"]
mod review_sets;

fn load_plan(f: &Fixture, command: PrMetadataCommand) -> PrMetadataPlan {
    PrMetadataPlan::prepare(
        load(f, command),
        command,
        if command == PrMetadataCommand::ClearMilestone {
            None
        } else {
            Some(0)
        },
    )
    .unwrap()
}
fn whole(command: PrMetadataCommand) -> Vec<GhStep> {
    let mut calls = catalogue(command, &parent(), vec![candidate(command)]);
    calls.extend(preparation(command));
    calls.extend(publication(command));
    calls
}
fn execute(f: &Fixture, plan: &PrMetadataPlan) -> PrMetadataExecution {
    let provider = provider(f);
    let mut journal = PrWriteJournal::open(&f.root.join("provider-metadata.sqlite")).unwrap();
    let limits = limits();
    let cancel = CancellationToken::default();
    plan.execute(
        PrMetadataExecutionContext {
            provider: &provider,
            journal: &mut journal,
            confirmed: true,
            restored: None,
            limits: &limits,
            cancel: &cancel,
        },
        |_| Ok(()),
        |_, _| {
            Ok(PrMetadataRefresh {
                private_visible: true,
            })
        },
    )
}

#[test]
fn pr_metadata_actions_provider_all_ten_effects_use_exact_id_native_payloads_and_26_calls() {
    for command in PrMetadataCommand::ALL {
        let f = Fixture::new();
        let calls = whole(command);
        f.scripted_gh(&calls);
        let plan = load_plan(&f, command);
        let result = execute(&f, &plan);
        let PrMetadataExecution::Confirmed {
            receipt,
            publication,
            ..
        } = result
        else {
            panic!("expected exact actual result for {command:?}")
        };
        assert_eq!(receipt.effect().node_id.as_deref(), Some("PR_7"));
        assert_eq!(publication.confirmed, Some(receipt.effect()));
        assert_commands(&f, 26, 1);
        let row = &journal(&f)[20];
        assert_eq!(
            row["args"],
            json!([
                "api",
                "graphql",
                "--hostname",
                "github.invalid",
                "--method",
                "POST",
                "--input",
                "-"
            ])
        );
        assert_eq!(
            row["stdin_hex"],
            calls[20]
                .stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        let input = mutation_input(command);
        assert!(
            input.get("title").is_none()
                && input.get("body").is_none()
                && input.get("baseRefName").is_none()
                && input.get("projectIds").is_none()
        );
        if command.aggregate_review_replacement() {
            assert_eq!(input["union"], false);
            assert!(input["userIds"].is_array() && input["teamIds"].is_array());
            assert_eq!(input.as_object().unwrap().len(), 4);
        } else if matches!(
            command,
            PrMetadataCommand::AddReviewer | PrMetadataCommand::AddTeam
        ) {
            assert_eq!(input["union"], true);
            assert_eq!(input.as_object().unwrap().len(), 3);
        } else {
            assert_eq!(input.as_object().unwrap().len(), 2);
        }
    }
}
#[test]
fn pr_metadata_actions_provider_aggregate_removal_last_user_last_team_sends_genuine_explicit_empty_sets(
) {
    for command in [
        PrMetadataCommand::RemoveReviewer,
        PrMetadataCommand::RemoveTeam,
    ] {
        let f = Fixture::new();
        let mut first = parent();
        first[if command == PrMetadataCommand::RemoveReviewer {
            "requested_teams"
        } else {
            "requested_reviewers"
        }] = json!([]);
        let mut last = first.clone();
        last[if command == PrMetadataCommand::RemoveReviewer {
            "requested_reviewers"
        } else {
            "requested_teams"
        }] = json!([]);
        let mut calls = catalogue(command, &first, vec![candidate(command)]);
        calls.extend(preparation_from(command, &first));
        calls.extend(publication_from(
            command,
            &last,
            json!({"pullRequestId":"PR_7","union":false,"userIds":[],"teamIds":[]}),
        ));
        f.scripted_gh(&calls);
        let plan = load_plan(&f, command);
        let result = execute(&f, &plan);
        assert!(matches!(result, PrMetadataExecution::Confirmed { .. }));
        assert_eq!(plan.after().len(), 0);
        assert_eq!(plan.preserved_review_members().len(), 0);
        assert_eq!(last["labels"], first["labels"]);
        assert_eq!(last["assignees"], first["assignees"]);
        assert_eq!(last["milestone"], first["milestone"]);
        assert_commands(&f, 26, 1);
    }
}
#[test]
fn pr_metadata_actions_provider_preclaim_current_connection_caps_nulls_unknown_actor_and_replaced_canonical_node_refuse(
) {
    for (command, fault) in [
        (PrMetadataCommand::AddLabel, "cap"),
        (PrMetadataCommand::AddLabel, "null"),
        (PrMetadataCommand::AddLabel, "node"),
        (PrMetadataCommand::AddReviewer, "actor"),
        (PrMetadataCommand::AddTeam, "namespace"),
    ] {
        let f = Fixture::new();
        let mut calls = whole(command);
        let mut current = members(command, &parent());
        let node = &mut current["data"]["node"];
        match fault {
            "cap" => node["labels"]["pageInfo"]["hasNextPage"] = json!(true),
            "null" => node["labels"] = Value::Null,
            "node" => node["repository"]["id"] = json!("R_recreated"),
            "actor" => {
                node["reviewRequests"]["nodes"][0]["requestedReviewer"]["__typename"] =
                    json!("Mannequin")
            }
            _ => {
                node["reviewRequests"]["nodes"][1]["requestedReviewer"]["organization"]["login"] =
                    json!("other")
            }
        }
        calls[16].stdout = serde_json::to_vec(&current).unwrap();
        calls.truncate(17);
        f.scripted_gh(&calls);
        let plan = load_plan(&f, command);
        let result = execute(&f, &plan);
        if matches!(fault, "node" | "namespace") {
            assert!(
                matches!(result, PrMetadataExecution::RefusedPrivate { .. }),
                "{fault}"
            );
        } else {
            assert!(
                matches!(result, PrMetadataExecution::Refused { .. }),
                "{fault}"
            );
        }
        assert_commands(&f, 17, 0);
        assert!(
            PrWriteJournal::open(&f.root.join("provider-metadata.sqlite"))
                .unwrap()
                .blocking(plan.target())
                .unwrap()
                .is_none()
        );
    }
}
#[test]
fn pr_metadata_actions_provider_readback_actual_numeric_and_opaque_member_ids_not_same_name_are_required(
) {
    let command = PrMetadataCommand::AddAssignee;
    for fault in ["db", "node", "missing", "count", "head", "uid"] {
        let f = Fixture::new();
        let mut calls = whole(command);
        match fault {
            "db" | "node" => {
                let mut issue = issue_from(&changed(command));
                issue["assignees"][1][if fault == "db" { "id" } else { "node_id" }] =
                    if fault == "db" {
                        json!(999)
                    } else {
                        json!("U_reused")
                    };
                calls[24].stdout = serde_json::to_vec(&issue).unwrap();
                calls.truncate(25);
            }
            "missing" | "count" => {
                let mut raw = members(command, &changed(command));
                if fault == "missing" {
                    raw["data"]["node"]["assignees"] = Value::Null;
                } else {
                    raw["data"]["node"]["assignees"]["totalCount"] = json!(3);
                }
                calls[21].stdout = serde_json::to_vec(&raw).unwrap();
                calls.truncate(22);
            }
            "head" => {
                let mut p = changed(command);
                p["head"]["sha"] = json!("c".repeat(40));
                calls[23].stdout = serde_json::to_vec(&p).unwrap();
                calls.truncate(24);
            }
            _ => {
                calls[25].stdout =
                    serde_json::to_vec(&json!({"id":43,"login":"requester"})).unwrap();
            }
        }
        let count = calls.len();
        f.scripted_gh(&calls);
        let plan = load_plan(&f, command);
        assert!(
            matches!(execute(&f, &plan), PrMetadataExecution::Uncertain { .. }),
            "{fault}"
        );
        assert_commands(&f, count, 1);
        assert!(
            PrWriteJournal::open(&f.root.join("provider-metadata.sqlite"))
                .unwrap()
                .blocking(plan.target())
                .unwrap()
                .is_some()
        );
    }
}
#[test]
fn pr_metadata_actions_provider_preclaim_closing_permissions_account_head_and_rest_concurrency_refuse(
) {
    let command = PrMetadataCommand::AddLabel;
    for fault in ["permission", "uid", "head", "field"] {
        let f = Fixture::new();
        let mut calls = whole(command);
        match fault {
            "permission" => {
                let mut r = repository();
                r["permissions"] = Value::Null;
                calls[17].stdout = serde_json::to_vec(&r).unwrap();
                calls.truncate(18);
            }
            "uid" => {
                calls[19].stdout =
                    serde_json::to_vec(&json!({"id":43,"login":"requester"})).unwrap();
                calls.truncate(20);
            }
            _ => {
                let mut p = parent();
                if fault == "head" {
                    p["head"]["sha"] = json!("d".repeat(40));
                } else {
                    p["labels"] = json!([label(90, "other-current")]);
                }
                calls[18].stdout = serde_json::to_vec(&p).unwrap();
                calls.truncate(19);
            }
        }
        let count = calls.len();
        f.scripted_gh(&calls);
        let plan = load_plan(&f, command);
        let result = execute(&f, &plan);
        if matches!(fault, "uid" | "permission") {
            assert!(
                matches!(result, PrMetadataExecution::RefusedPrivate { .. }),
                "{fault}"
            );
        } else {
            assert!(
                matches!(result, PrMetadataExecution::Refused { .. }),
                "{fault}"
            );
        }
        assert_commands(&f, count, 0);
        assert!(
            PrWriteJournal::open(&f.root.join("provider-metadata.sqlite"))
                .unwrap()
                .blocking(plan.target())
                .unwrap()
                .is_none()
        );
    }
}
#[test]
fn pr_metadata_actions_provider_precancel_and_invalid_limits_do_not_launch_fresh_preparation() {
    let command = PrMetadataCommand::AddLabel;
    let f = Fixture::new();
    f.scripted_gh(&catalogue(command, &parent(), vec![candidate(command)]));
    let plan = load_plan(&f, command);
    let cancel = CancellationToken::default();
    cancel.cancel();
    assert!(provider(&f)
        .prepare_metadata_publication(&plan, &limits(), &cancel)
        .is_err());
    assert_commands(&f, 8, 0);
    for limits in [
        PrMetadataReadLimits {
            max_response_bytes: 0,
            ..limits()
        },
        PrMetadataReadLimits {
            max_duration: Duration::ZERO,
            ..limits()
        },
        PrMetadataReadLimits {
            max_document_bytes: 0,
            ..limits()
        },
    ] {
        assert!(provider(&f)
            .prepare_metadata_publication(&plan, &limits, &CancellationToken::default())
            .is_err());
        assert_commands(&f, 8, 0);
    }
}
#[test]
fn pr_metadata_actions_provider_failed_capture_and_multibyte_diagnostics_are_reserved_before_later_calls(
) {
    let command = PrMetadataCommand::AddLabel;
    for huge in [false, true] {
        let f = Fixture::new();
        let mut calls = whole(command);
        // A failed connection is never authority and never followed by closing
        // reads or a write. Real capture overflow uses both raw pipes.
        calls[16].stdout = if huge {
            "界".repeat(50000).into_bytes()
        } else {
            vec![]
        };
        calls[16].stderr = if huge {
            "\u{001b}".repeat(50000).into_bytes()
        } else {
            b"HTTP 403 catalogue capability".to_vec()
        };
        calls[16].status = 1;
        calls.truncate(17);
        f.scripted_gh(&calls);
        let plan = load_plan(&f, command);
        let result = provider(&f).prepare_metadata_publication(
            &plan,
            &limits(),
            &CancellationToken::default(),
        );
        assert!(result.is_err());
        assert_commands(&f, 17, 0);
    }
}
#[test]
fn pr_metadata_actions_provider_shared_success_capture_budget_and_claim_adjacent_original_deadline_are_not_reset(
) {
    let command = PrMetadataCommand::AddLabel;
    let f = Fixture::new();
    let mut calls = whole(command);
    // Actual full raw stderr would sanitize away. The preclaim GraphQL command
    // consumes it under the SAME budget as all eight fresh catalogue commands.
    calls[16].stderr = vec![0x1b; 4096];
    f.scripted_gh(&calls);
    let plan = load_plan(&f, command);
    let fresh_bytes: usize = calls[8..17]
        .iter()
        .map(|s| s.stdout.len() + s.stderr.len())
        .sum();
    let chosen = PrMetadataReadLimits {
        max_response_bytes: fresh_bytes,
        max_document_bytes: 128 * 1024,
        ..limits()
    };
    let result =
        provider(&f).prepare_metadata_publication(&plan, &chosen, &CancellationToken::default());
    assert!(result.is_err());
    assert_commands(&f, 17, 0);
    assert!(matches!(
        result.unwrap_err().kind,
        GithubErrorKind::Transport
    ));
}
