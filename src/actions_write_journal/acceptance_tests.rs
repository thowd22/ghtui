use super::*;

#[test]
fn actions_run_control_journal_distinct_logs_acceptance_ack_is_unverified_and_permits_only_a_new_attempt(
) {
    let fixture = Fixture::new();
    let database = fixture.repo().join("cache.sqlite3");
    let mut steps = run_inspection();
    steps.extend(run_inspection());
    steps.extend(account(42));
    fixture.scripted_gh(&steps);
    let client = gh(&fixture, Default::default());
    let target =
        ActionsWriteTarget::new(&client, &identity(Some(42)), &run("example/repo")).unwrap();
    let value = intent(ActionsWriteKind::DeleteLogs, b"original log deletion");
    let mut journal = ActionsWriteJournal::open(&database).unwrap();
    let permit = journal.begin(&target, &value).unwrap();
    journal.claim(&permit, &target, &value).unwrap();
    let original = receipt(ActionsReceiptKind::LogsDeleted);
    journal.mark_accepted_pending(&permit, &original).unwrap();
    let reference = permit.marker().publication_reference();
    let ordinary = journal
        .prepare_inspection(
            &client,
            &identity(Some(42)),
            reference.attempt_id,
            &CancellationToken::default(),
        )
        .unwrap();
    let disposition = ordinary.disposition().unwrap();
    assert_eq!(
        disposition,
        ActionsInspectionDisposition::AcceptedEffectUnverified
    );
    assert!(!disposition.can_acknowledge());
    assert!(disposition.can_acknowledge_inspected_acceptance());
    assert!(journal
        .acknowledge(
            &client,
            ordinary,
            &identity(Some(42)),
            &CancellationToken::default()
        )
        .is_err());
    let distinct = journal
        .prepare_inspection(
            &client,
            &identity(Some(42)),
            reference.attempt_id,
            &CancellationToken::default(),
        )
        .unwrap();
    journal
        .acknowledge_inspected_acceptance(
            &client,
            distinct,
            &identity(Some(42)),
            &CancellationToken::default(),
        )
        .unwrap();
    let marker = journal.restored_publication(&target, &reference).unwrap();
    assert_eq!(marker.state, ActionsWriteState::Acknowledged);
    assert_ne!(marker.state, ActionsWriteState::Confirmed);
    assert_ne!(marker.state, ActionsWriteState::TransitionObserved);
    assert_eq!(
        marker.receipt,
        Some(original),
        "retain original acceptance, never fabricate a deletion receipt"
    );
    assert_only_reads(&fixture, 12);
    let next = journal.begin(&target, &value).unwrap();
    assert_ne!(
        next.marker().attempt_id,
        reference.attempt_id,
        "distinct user confirmation yields a fresh attempt, not an automatic original resend"
    );
    assert_only_reads(&fixture, 12);
}

#[test]
fn workflow_controls_journal_distinct_dispatch_acceptance_rejects_pending_or_known_correlation() {
    for mode in ["uncorrelated", "correlated-missing", "pending"] {
        let fixture = Fixture::new();
        let database = fixture.repo().join("cache.sqlite3");
        let mut steps = account(42);
        steps.push(read("repos/example/repo/actions/workflows?per_page=100",json!([{"workflows":[{"id":42,"name":"CI","path":".github/workflows/ci.yml","state":"active"}]}]),true));
        steps.push(read(
            "repos/example/repo/actions/workflows/42/runs?per_page=100",
            json!([{"total_count":10,"workflow_runs":[]}]),
            true,
        ));
        steps.extend(account(42));
        if mode == "uncorrelated" {
            steps.extend(account(42));
        }
        fixture.scripted_gh(&steps);
        let client = gh(&fixture, Default::default());
        let target = ActionsWriteTarget::new(
            &client,
            &identity(Some(42)),
            &ActionsTarget::Workflow {
                repository: "example/repo".into(),
                id: 42,
            },
        )
        .unwrap();
        let value = ActionsWriteIntent::new(
            ActionsWriteKind::DispatchWorkflow,
            ActionsImmutableContext {
                workflow_commit: Some("a".repeat(40)),
                ..Default::default()
            },
            Default::default(),
            b"transient private dispatch inputs",
        )
        .unwrap();
        let mut journal = ActionsWriteJournal::open(&database).unwrap();
        let permit = journal.begin(&target, &value).unwrap();
        if mode != "pending" {
            journal.claim(&permit, &target, &value).unwrap();
            journal
                .mark_accepted_pending(
                    &permit,
                    &ActionsReceipt {
                        kind: ActionsReceiptKind::Dispatch,
                        resource_id: Some(42),
                        observed_attempt: None,
                        observed_run_id: if mode == "correlated-missing" {
                            Some(123)
                        } else {
                            None
                        },
                    },
                )
                .unwrap();
        }
        let inspection = journal
            .prepare_inspection(
                &client,
                &identity(Some(42)),
                permit.marker().attempt_id,
                &CancellationToken::default(),
            )
            .unwrap();
        let disposition = inspection.disposition().unwrap();
        assert_eq!(
            disposition,
            match mode {
                "uncorrelated" => ActionsInspectionDisposition::AcceptedEffectUnverified,
                "pending" => ActionsInspectionDisposition::Pending {
                    state: ActionsWriteState::Pending
                },
                _ => ActionsInspectionDisposition::AcceptedStillPending,
            }
        );
        assert_eq!(disposition.can_acknowledge(), mode == "pending");
        assert_eq!(
            disposition.can_acknowledge_inspected_acceptance(),
            mode == "uncorrelated"
        );
        let result = journal.acknowledge_inspected_acceptance(
            &client,
            inspection,
            &identity(Some(42)),
            &CancellationToken::default(),
        );
        let marker = read_attempt(
            &journal.connection,
            target.scope(),
            permit.marker().attempt_id,
        )
        .unwrap()
        .unwrap();
        if mode == "uncorrelated" {
            result.unwrap();
            assert_eq!(marker.state, ActionsWriteState::Acknowledged);
            assert_eq!(marker.receipt.unwrap().observed_run_id, None);
            assert_only_reads(&fixture, 8);
        } else {
            assert!(result.is_err());
            assert_eq!(
                marker.state,
                if mode == "pending" {
                    ActionsWriteState::Pending
                } else {
                    ActionsWriteState::AcceptedPending
                }
            );
            assert!(!journal
                .blocking(&target, ActionsWriteKind::DispatchWorkflow)
                .unwrap()
                .is_empty());
            assert_only_reads(&fixture, 6);
        }
    }
}
