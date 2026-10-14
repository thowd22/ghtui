//! Activated guarded metadata contracts. TASK-18 records actual code/provider
//! and real-terminal results; backend gates do not establish whole workflow parity.
use super::*;
use crate::providers::{gh::PrMetadataReadLimits, runner::CancellationToken};
use crate::{
    self as model,
    pr_write_journal::{PrWriteJournal, PrWriteState},
    test_support::{Fixture, GhStep},
};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::{fs, process::Command, sync::Arc, time::Duration};
#[path = "../tests/support/metadata_actions.rs"]
mod fixture;
use fixture::*;
#[path = "pr_metadata_actions_native_tests.rs"]
mod native;
#[path = "pr_metadata_actions_label_guard_tests.rs"]
mod read_admission;
#[path = "pr_metadata_actions_recovery_tests.rs"]
mod recovery;
#[path = "pr_metadata_actions_invalidation_tests.rs"]
mod structured;

fn plan(f: &Fixture, command: PrMetadataCommand) -> PrMetadataPlan {
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
fn setup(command: PrMetadataCommand) -> (Fixture, PrMetadataPlan, PrWriteJournal) {
    let f = Fixture::new();
    let mut steps = catalogue(command, &parent(), vec![candidate(command)]);
    steps.extend(preparation(command));
    steps.extend(publication(command));
    f.scripted_gh(&steps);
    let plan = plan(&f, command);
    let journal = PrWriteJournal::open(&f.root.join("metadata.sqlite")).unwrap();
    (f, plan, journal)
}
fn execution_context<'a>(
    journal: &'a mut PrWriteJournal,
    provider: &'a crate::providers::gh::GithubProvider,
    limits: &'a PrMetadataReadLimits,
    cancel: &'a CancellationToken,
) -> PrMetadataExecutionContext<'a> {
    PrMetadataExecutionContext {
        provider,
        journal,
        confirmed: true,
        restored: None,
        limits,
        cancel,
    }
}
fn known_refresh(
    _: &PrMetadataReceipt,
    _: &PrMetadataStructuredImpact,
) -> Result<PrMetadataRefresh, String> {
    Ok(PrMetadataRefresh {
        private_visible: true,
    })
}
fn assert_sql(journal: &PrWriteJournal, plan: &PrMetadataPlan, state: &str, count: usize) {
    let connection = Connection::open(journal.database_path()).unwrap();
    let (actual,n):(String,usize)=connection.query_row("SELECT state,count(*) FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3",params!["github.invalid","user-id:42",plan.original().pr.id],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
    assert_eq!((actual.as_str(), n), (state, count));
}
#[test]
fn pr_metadata_actions_all_ten_plans_use_genuine_indexed_values_and_single_field_payloads() {
    for command in PrMetadataCommand::ALL {
        let f = Fixture::new();
        f.scripted_gh(&catalogue(command, &parent(), vec![candidate(command)]));
        let plan = plan(&f, command);
        assert_eq!(plan.target().scope().login, "user-id:42");
        assert_eq!(plan.original().pr.id, "PR_7");
        assert_eq!(
            plan.selected().unwrap().id(),
            match command {
                PrMetadataCommand::RemoveLabel => 80,
                PrMetadataCommand::RemoveAssignee => 60,
                PrMetadataCommand::RemoveReviewer => 70,
                PrMetadataCommand::RemoveTeam => 75,
                PrMetadataCommand::ClearMilestone => 85,
                _ => candidate(command)["id"].as_u64().unwrap(),
            }
        );
        assert_eq!(plan.mutation_input().unwrap(), mutation_input(command));
        if command.aggregate_review_replacement() {
            assert!(command
                .operation_notice()
                .contains("COMPLETE current User+Team"));
            assert_eq!(plan.preserved_review_members().len(), 1);
        }
        assert!(!format!("{plan:?}").contains("Private description"));
        assert_commands(&f, 8, 0);
    }
}
#[test]
fn pr_metadata_actions_nullable_current_sets_nodes_permissions_and_milestone_never_clear_by_default(
) {
    for field in ["labels", "assignees", "milestone"] {
        for missing in [true, false] {
            let command = match field {
                "labels" => PrMetadataCommand::RemoveLabel,
                "assignees" => PrMetadataCommand::RemoveAssignee,
                _ => PrMetadataCommand::ClearMilestone,
            };
            let f = Fixture::new();
            let mut p = parent();
            if missing {
                p.as_object_mut().unwrap().remove(field);
            } else {
                p[field] = json!({"unknown":true});
            }
            f.scripted_gh(&catalogue(command, &p, vec![candidate(command)]));
            let source = load(&f, command);
            assert!(PrMetadataPlan::prepare(
                source,
                command,
                if command == PrMetadataCommand::ClearMilestone {
                    None
                } else {
                    Some(0)
                }
            )
            .is_err());
            assert_commands(&f, 8, 0);
        }
    }
    for invalid in [json!(null), json!(""), json!(81)] {
        let f = Fixture::new();
        let command = PrMetadataCommand::AddLabel;
        let mut row = candidate(command);
        row["node_id"] = invalid;
        f.scripted_gh(&catalogue(command, &parent(), vec![row]));
        assert!(PrMetadataPlan::prepare(load(&f, command), command, Some(0)).is_err());
        assert_commands(&f, 8, 0);
    }
}
#[test]
fn pr_metadata_actions_candidate_self_author_namespace_nullable_capability_and_reused_names_refuse()
{
    for (command, row) in [
        (PrMetadataCommand::AddReviewer, user(51, "author-renamed")),
        (PrMetadataCommand::AddTeam, {
            let mut v = team(76, "new-team");
            v["organization"]["login"] = json!("other");
            v
        }),
        (PrMetadataCommand::AddReviewer, {
            let mut v = user(71, "new-reviewer");
            v["permissions"] = json!(null);
            v
        }),
        (PrMetadataCommand::AddLabel, label(81, "existing ☃")),
    ] {
        let f = Fixture::new();
        f.scripted_gh(&catalogue(command, &parent(), vec![row]));
        assert!(PrMetadataPlan::prepare(load(&f, command), command, Some(0)).is_err());
        assert_commands(&f, 8, 0);
    }
    let f = Fixture::new();
    let command = PrMetadataCommand::AddLabel;
    let mut calls = catalogue(command, &parent(), vec![candidate(command)]);
    calls[5].stdout = serde_json::to_vec(&{
        let mut r = repository();
        r["permissions"] = Value::Null;
        r
    })
    .unwrap();
    f.scripted_gh(&calls);
    assert!(PrMetadataPlan::prepare(load(&f, command), command, Some(0))
        .unwrap_err()
        .contains("capability"));
    assert_commands(&f, 8, 0);
}
#[test]
fn pr_metadata_actions_partial_choice_is_not_complete_current_set_or_missing_member_authority() {
    let command = PrMetadataCommand::AddLabel;
    let f = Fixture::new();
    let rows = vec![candidate(command), label(82, "another")];
    let calls = catalogue(command, &parent(), rows.clone());
    f.scripted_gh(&calls);
    let (target, context) = original();
    let source = Arc::new(
        provider(&f)
            .metadata_catalogue_read(
                &identity(),
                &target,
                &context,
                command.catalogue(),
                &PrMetadataReadLimits {
                    max_pages: 1,
                    ..limits()
                },
                &CancellationToken::default(),
            )
            .unwrap(),
    );
    assert!(!source.coverage().enumeration_complete);
    let plan = PrMetadataPlan::prepare(source.clone(), command, Some(0)).unwrap();
    assert_eq!(plan.after().len(), 2);
    assert!(PrMetadataPlan::prepare(source, PrMetadataCommand::RemoveLabel, Some(99)).is_err());
    assert_commands(&f, 8, 0);
}
#[test]
fn pr_metadata_actions_fresh_actual_ids_current_set_head_base_and_candidate_revalidation_is_atomic()
{
    let command = PrMetadataCommand::AddLabel;
    for fault in ["current", "candidate", "author", "issue", "permission"] {
        let f = Fixture::new();
        let mut calls = catalogue(command, &parent(), vec![candidate(command)]);
        let mut p = parent();
        let mut row = candidate(command);
        match fault {
            "current" => p["labels"] = json!([label(80, "existing ☃"), label(89, "concurrent")]),
            "candidate" => row["id"] = json!(999),
            "author" => p["user"] = user(52, "author"),
            _ => {}
        }
        let mut later = catalogue(command, &p, vec![row]);
        if fault == "issue" {
            let mut v = issue_from(&p);
            v["id"] = json!(9999);
            later[3].stdout = serde_json::to_vec(&v).unwrap();
        }
        if fault == "permission" {
            let mut v = repository();
            v["permissions"]["push"] = json!(false);
            later[5].stdout = serde_json::to_vec(&v).unwrap();
        }
        calls.extend(later);
        f.scripted_gh(&calls);
        let plan = plan(&f, command);
        let fresh = load(&f, command);
        assert!(plan.validate_fresh(&fresh).is_err(), "{fault}");
        assert_commands(&f, 16, 0);
    }
}
#[test]
fn pr_metadata_actions_receipts_are_closed_ascii_actual_ids_and_reject_forged_semantics() {
    for command in [
        PrMetadataCommand::AddLabel,
        PrMetadataCommand::RemoveLabel,
        PrMetadataCommand::SetMilestone,
        PrMetadataCommand::ClearMilestone,
    ] {
        let f = Fixture::new();
        f.scripted_gh(&catalogue(command, &parent(), vec![candidate(command)]));
        let plan = plan(&f, command);
        let receipt = plan.receipt().unwrap();
        let effect = receipt.effect();
        assert_eq!(PrMetadataReceipt::decode(&effect).unwrap(), receipt);
        assert!(effect.resource_id.is_ascii() && effect.resource_id.len() <= 4096);
        assert!(!effect.resource_id.contains("☃") && !effect.resource_id.contains("description"));
        for replacement in [
            "metadata-v1/arbitrary/410/1007/2007/81/unknown/unknown/empty",
            "metadata-v1/metadata-add-label/0/1007/2007/81/unknown/unknown/empty",
        ] {
            let mut forged = effect.clone();
            forged.resource_id = replacement.into();
            assert!(PrMetadataReceipt::decode(&forged).is_err());
        }
        let mut wrong = effect.clone();
        wrong.node_id = Some("not a node".into());
        assert!(PrMetadataReceipt::decode(&wrong).is_err());
        assert_commands(&f, 8, 0);
    }
}
#[test]
fn pr_metadata_actions_sql_confirmed_receipt_precedes_optional_refresh_and_original_restore_ignores_edited_head(
) {
    let (f, plan, mut journal) = setup(PrMetadataCommand::AddLabel);
    let p = provider(&f);
    let limits = limits();
    let cancel = CancellationToken::default();
    let database = journal.database_path().to_owned();
    let result = plan.execute(
        execution_context(&mut journal, &p, &limits, &cancel),
        |marker| {
            assert_eq!(marker.state, PrWriteState::Pending);
            assert!(marker.confirmed.is_none());
            Ok(())
        },
        |receipt, impact| {
            assert_eq!(impact.target().scope().login, "user-id:42");
            assert_eq!(impact.source_epoch(), 3);
            assert!(impact
                .notice()
                .contains("Current PR metadata and Dashboard"));
            let other = PrWriteJournal::open(&database).unwrap();
            let saved = other
                .latest_matching(plan.target(), plan.intent())
                .unwrap()
                .unwrap();
            assert_eq!(saved.state, PrWriteState::Confirmed);
            assert_eq!(saved.confirmed, Some(receipt.effect()));
            Ok(PrMetadataRefresh {
                private_visible: true,
            })
        },
    );
    let PrMetadataExecution::Confirmed {
        receipt,
        publication,
        ..
    } = result
    else {
        panic!("expected actual typed confirmation")
    };
    assert_eq!(receipt.result_ids(), &[(80, None), (81, None)]);
    assert_sql(&journal, &plan, "confirmed", 1);
    let mut edited = plan.original().clone();
    edited.head_sha = "bad current edited head".into();
    edited.pr.body = "New arbitrary edited body".into();
    let restored = restore_original(
        &journal,
        &identity(),
        &edited,
        &publication.publication_reference(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(restored.attempt_id, publication.attempt_id);
    assert_commands(&f, 26, 1);
}
#[test]
fn pr_metadata_actions_sql_ordered_flush_failure_retains_pending_and_never_dispatches() {
    let (f, plan, mut journal) = setup(PrMetadataCommand::AddAssignee);
    let p = provider(&f);
    let limits = limits();
    let cancel = CancellationToken::default();
    let result = plan.execute(
        execution_context(&mut journal, &p, &limits, &cancel),
        |_| Err("actual draft SQL unavailable".into()),
        known_refresh,
    );
    assert!(matches!(result, PrMetadataExecution::Refused { .. }));
    assert_sql(&journal, &plan, "pending", 1);
    assert_commands(&f, 20, 0);
    let blocked = plan.execute(
        execution_context(&mut journal, &p, &limits, &cancel),
        |_| Ok(()),
        known_refresh,
    );
    assert!(matches!(blocked, PrMetadataExecution::Refused { .. }));
    assert_commands(&f, 20, 0);
}
#[test]
fn pr_metadata_actions_sql_receipt_failure_retains_known_effect_and_original_no_resend_barrier() {
    let (f, plan, mut journal) = setup(PrMetadataCommand::SetMilestone);
    let database = journal.database_path().to_owned();
    Connection::open(&database).unwrap().execute_batch("CREATE TRIGGER reject_metadata_receipt BEFORE UPDATE OF state ON pr_write_attempts WHEN NEW.state='confirmed' BEGIN SELECT RAISE(ABORT,'receipt SQL failed'); END;").unwrap();
    let p = provider(&f);
    let limits = limits();
    let cancel = CancellationToken::default();
    let result = plan.execute(
        execution_context(&mut journal, &p, &limits, &cancel),
        |_| Ok(()),
        |_, _| panic!("optional refresh must not precede durable receipt"),
    );
    let PrMetadataExecution::ConfirmedButJournalFailed { receipt, .. } = result else {
        panic!("expected actual receipt SQL failure")
    };
    assert_eq!(receipt.result_ids(), &[(86, Some(6))]);
    assert_sql(&journal, &plan, "pending", 1);
    assert_commands(&f, 26, 1);
    let again = plan.execute(
        execution_context(&mut journal, &p, &limits, &cancel),
        |_| Ok(()),
        known_refresh,
    );
    assert!(matches!(again, PrMetadataExecution::Refused { .. }));
    assert_commands(&f, 26, 1);
}
#[test]
fn pr_metadata_actions_confirmed_refresh_failure_recovery_reads_receipt_without_resending_or_source_reload(
) {
    let (f, plan, mut journal) = setup(PrMetadataCommand::ClearMilestone);
    let p = provider(&f);
    let limits = limits();
    let cancel = CancellationToken::default();
    let result = plan.execute(
        execution_context(&mut journal, &p, &limits, &cancel),
        |_| Ok(()),
        |_, _| Err("Optional view refresh failed".into()),
    );
    let PrMetadataExecution::ConfirmedButRefreshFailed {
        publication,
        private_visible: false,
        ..
    } = result
    else {
        panic!("expected typed optional refresh failure")
    };
    assert_sql(&journal, &plan, "confirmed", 1);
    assert_commands(&f, 26, 1);
    let reference = publication.publication_reference();
    let recovered = plan.execute(
        PrMetadataExecutionContext {
            provider: &p,
            journal: &mut journal,
            confirmed: false,
            restored: Some(&reference),
            limits: &limits,
            cancel: &cancel,
        },
        |_| panic!("no new original flush"),
        |_, _| panic!("no reloading receipt recovery"),
    );
    assert!(matches!(
        recovered,
        PrMetadataExecution::RestoredAlreadyConfirmed { .. }
    ));
    assert_commands(&f, 26, 1);
}
#[test]
fn pr_metadata_actions_uncertain_result_and_positive_uid_drift_block_repeated_publication() {
    for failed in [true, false] {
        let command = PrMetadataCommand::AddReviewer;
        let f = Fixture::new();
        let mut calls = catalogue(command, &parent(), vec![candidate(command)]);
        calls.extend(preparation(command));
        let mut write = publication(command);
        if failed {
            write[0].status = 1;
            write[0].stderr = b"transport failed after invocation".to_vec();
            write.truncate(1);
        } else {
            write[5].stdout = serde_json::to_vec(&json!({"id":43,"login":"requester"})).unwrap();
        }
        calls.extend(write);
        f.scripted_gh(&calls);
        let plan = plan(&f, command);
        let mut journal = PrWriteJournal::open(&f.root.join("metadata.sqlite")).unwrap();
        let p = provider(&f);
        let limits = limits();
        let cancel = CancellationToken::default();
        let result = plan.execute(
            execution_context(&mut journal, &p, &limits, &cancel),
            |_| Ok(()),
            |_, _| panic!("uncertain cannot refresh private view"),
        );
        assert!(matches!(result, PrMetadataExecution::Uncertain { .. }));
        assert_sql(&journal, &plan, "uncertain", 1);
        assert_commands(&f, if failed { 21 } else { 26 }, 1);
        assert!(matches!(
            plan.execute(
                execution_context(&mut journal, &p, &limits, &cancel),
                |_| Ok(()),
                known_refresh
            ),
            PrMetadataExecution::Refused { .. }
        ));
        assert_commands(&f, if failed { 21 } else { 26 }, 1);
    }
}
#[test]
fn pr_metadata_actions_deadline_after_ordered_flush_prevents_claim_write_and_preserves_real_git() {
    let (f, plan, mut journal) = setup(PrMetadataCommand::AddLabel);
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .current_dir(f.repo())
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    };
    git(&["init", "-q"]);
    git(&["config", "user.name", "Fixture"]);
    git(&["config", "user.email", "fixture@example.invalid"]);
    fs::write(f.repo().join("tracked.txt"), b"baseline\n").unwrap();
    git(&["add", "tracked.txt"]);
    git(&["commit", "-qm", "baseline"]);
    fs::write(f.repo().join("tracked.txt"), b"private dirty bytes\n").unwrap();
    let status = git(&["status", "--porcelain=v1"]);
    let head = git(&["rev-parse", "HEAD"]);
    let p = provider(&f);
    let limits = PrMetadataReadLimits {
        max_duration: Duration::from_secs(1),
        ..limits()
    };
    let cancel = CancellationToken::default();
    let result = plan.execute(
        execution_context(&mut journal, &p, &limits, &cancel),
        |_| {
            std::thread::sleep(Duration::from_millis(1100));
            Ok(())
        },
        known_refresh,
    );
    assert!(matches!(result, PrMetadataExecution::Refused { .. }));
    assert_sql(&journal, &plan, "pending", 1);
    assert_commands(&f, 20, 0);
    assert_eq!(git(&["status", "--porcelain=v1"]), status);
    assert_eq!(git(&["rev-parse", "HEAD"]), head);
    assert!(git(&["diff", "--cached"]).is_empty());
    assert_eq!(
        fs::read(f.repo().join("tracked.txt")).unwrap(),
        b"private dirty bytes\n"
    );
}
