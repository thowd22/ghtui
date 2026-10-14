use super::*;

#[test]
fn actions_scope_target_is_pure_and_matches_verified_worker_aliases() {
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let targets = [
        run("Example/Repo"),
        ActionsTarget::Workflow {
            repository: "Example/Repo".into(),
            id: 7,
        },
        ActionsTarget::DeploymentReview {
            repository: "Example/Repo".into(),
            run_id: 123,
            attempt: 3,
            head_sha: "a".repeat(40),
            environment_ids: vec![11, 10],
        },
        ActionsTarget::Secret {
            scope: ActionsScope::Organization {
                organization: "Example".into(),
            },
            name: "TOKEN".into(),
        },
        ActionsTarget::Variable {
            scope: ActionsScope::Environment {
                repository: "Example/Repo".into(),
                environment: "Production".into(),
            },
            name: "URL".into(),
        },
        ActionsTarget::GroupAssociations {
            organization: "Example".into(),
            id: 8,
            kind: GroupAssociation::Repositories,
        },
    ];
    for resource in targets {
        let retained = ActionsWriteTarget::from_scope(&scope, &resource).unwrap();
        let worker = ActionsWriteTarget::new(&provider(), &identity(Some(42)), &resource).unwrap();
        assert_eq!(retained.scope(), worker.scope());
        assert_eq!(retained.resource_keys(), worker.resource_keys());
        assert_eq!(
            serde_json::to_value(retained.target()).unwrap(),
            serde_json::to_value(&resource).unwrap()
        );
    }
    let dir = directory();
    let mut journal = ActionsWriteJournal::open(&dir.path().join("cache.sqlite3")).unwrap();
    let original = ActionsWriteTarget::from_scope(&scope, &run("Example/Repo")).unwrap();
    let permit = journal
        .begin(&original, &intent(ActionsWriteKind::Cancel, b""))
        .unwrap();
    let different_host = ActionsWriteTarget::from_scope(
        &Scope::github("other.invalid", Some(42)).unwrap(),
        &run("Example/Repo"),
    )
    .unwrap();
    assert!(!permit
        .marker()
        .blocks_target(&different_host, ActionsWriteKind::Cancel));
    assert!(permit
        .marker()
        .blocks_target(&original, ActionsWriteKind::Cancel));
    permit.marker().validate_metadata().unwrap();
    let mut forged = permit.marker().clone();
    forged.state = ActionsWriteState::AcceptedPending;
    assert!(forged
        .validate_metadata()
        .unwrap_err()
        .to_string()
        .contains("no typed receipt"));
    forged.receipt = Some(receipt(ActionsReceiptKind::RunTransition));
    forged.validate_metadata().unwrap();
    forged.receipt.as_mut().unwrap().resource_id = Some(999);
    assert!(forged.validate_metadata().is_err());
    forged.state = ActionsWriteState::Uncertain;
    forged.receipt = Some(receipt(ActionsReceiptKind::RunTransition));
    assert!(forged
        .validate_metadata()
        .unwrap_err()
        .to_string()
        .contains("Unconfirmed"));
}

#[test]
fn actions_scope_target_rejects_unverified_scope_invalid_resource_and_oversized_alias() {
    for (host, principal) in [
        ("github.invalid", "octocat"),
        ("github.invalid", "user-id:0"),
        ("github.invalid", "user-id:042"),
        ("github.invalid", "user-id:+42"),
        ("GITHUB.INVALID", "user-id:42"),
        ("github.invalid/path", "user-id:42"),
    ] {
        assert!(ActionsWriteTarget::from_scope(
            &Scope::new(host, principal).unwrap(),
            &run("example/repo")
        )
        .is_err());
    }
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    assert!(ActionsWriteTarget::from_scope(
        &scope,
        &ActionsTarget::Run {
            repository: "example/repo".into(),
            id: 0
        }
    )
    .is_err());
    assert!(ActionsWriteTarget::from_scope(
        &scope,
        &ActionsTarget::Run {
            repository: "../repo".into(),
            id: 123
        }
    )
    .is_err());
    let deployment = ActionsWriteTarget::from_scope(
        &scope,
        &ActionsTarget::DeploymentReview {
            repository: "example/repo".into(),
            run_id: 123,
            attempt: 3,
            head_sha: "a".repeat(40),
            environment_ids: (1..=17).collect(),
        },
    )
    .unwrap();
    assert_eq!(
        deployment.resource_keys(),
        &["github.invalid/example/repo/run/123"]
    );
    assert!(ActionsWriteTarget::from_scope(
        &scope,
        &ActionsTarget::Environment {
            repository: "example/repo".into(),
            name: "x".repeat(4097)
        }
    )
    .is_err());
}
