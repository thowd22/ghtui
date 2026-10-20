use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn gh_provider_inventory_contracts_all_builtin_categories_cover_exact_pr_and_actions_rows() {
    let expected: &[(&str, &[&str])] = &[
        ("personal-queues", &["pr.queues"]),
        ("pr-details", &["pr.details"]),
        ("pr-checks", &["pr.checks"]),
        ("pr-create", &["pr.create"]),
        ("pr-metadata", &["pr.metadata", "pr.dismissal"]),
        ("pr-lifecycle", &["pr.lifecycle"]),
        ("pr-lock-update-revert", &["pr.lifecycle", "pr.revert"]),
        ("pr-comments", &["pr.conversation"]),
        ("pr-comment-edit-delete-react", &["pr.conversation"]),
        ("pr-inline", &["pr.inline"]),
        ("pr-reviews", &["pr.pending-review"]),
        ("pr-pending-reviews", &["pr.pending-review", "pr.dismissal"]),
        ("pr-threads", &["pr.thread-state"]),
        ("pr-viewed", &["pr.thread-state"]),
        ("pr-suggestions", &["pr.suggestions"]),
        ("pr-conflicts", &["pr.conflicts"]),
        ("pr-merge", &["pr.merge"]),
        ("pr-queue", &["pr.merge"]),
        ("pr-navigation", &["pr.navigation"]),
        ("actions-workflows", &["actions.discovery"]),
        ("actions-details", &["actions.details"]),
        ("actions-logs", &["actions.logs"]),
        ("actions-dispatch", &["actions.dispatch"]),
        ("actions-control", &["actions.control"]),
        ("actions-delete", &["actions.control"]),
        ("actions-artifacts", &["actions.artifacts"]),
        ("actions-approval", &["actions.approvals"]),
        ("actions-cache", &["actions.cache"]),
        ("actions-secrets-variables", &["actions.settings"]),
        ("actions-policy", &["actions.environments"]),
        ("actions-environments", &["actions.environments"]),
        ("actions-runners", &["actions.runners"]),
        ("actions-activity", &["actions.activity"]),
    ];
    let inventory = super::super::capability_inventory();
    let actual = inventory
        .iter()
        .map(|c| {
            (
                c.id.as_str(),
                c.requirements_v1()
                    .unwrap()
                    .rows
                    .iter()
                    .map(|r| r.key())
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        actual,
        expected
            .iter()
            .map(|(id, rows)| (*id, rows.to_vec()))
            .collect()
    );
    let covered = actual.values().flatten().copied().collect::<BTreeSet<_>>();
    assert_eq!(covered.len(), 28);
    assert_eq!(
        covered,
        CapabilityContractRowV1::ALL
            .iter()
            .map(|r| r.key())
            .collect()
    );
    assert_eq!(
        CapabilityContractRowV1::ALL
            .iter()
            .filter(|r| r.document() == "doc-3")
            .count(),
        16
    );
    assert_eq!(
        CapabilityContractRowV1::ALL
            .iter()
            .filter(|r| r.document() == "doc-5")
            .count(),
        12
    );
}

#[test]
fn gh_provider_inventory_contracts_effects_tasks_and_target_guards_do_not_conflate_workflows() {
    use CapabilityEffectV1::*;
    for (id, effect, tasks, guard) in [
        (
            "personal-queues",
            ReadOnly,
            &[12u16][..],
            "queue memberships",
        ),
        ("pr-create", RemoteWrite, &[26][..], "canonical new PR"),
        ("pr-metadata", RemoteWrite, &[18, 20][..], "expected head"),
        (
            "pr-suggestions",
            IsolatedGitWrite,
            &[23][..],
            "isolated worktree/branch",
        ),
        (
            "pr-conflicts",
            IsolatedGitWrite,
            &[27][..],
            "user's checkout",
        ),
        (
            "pr-navigation",
            LocalOutput,
            &[10][..],
            "explicit local destination",
        ),
        (
            "actions-dispatch",
            Mixed,
            &[54][..],
            "immutable definition SHA",
        ),
        (
            "actions-artifacts",
            Mixed,
            &[56][..],
            "never execute content",
        ),
        (
            "actions-approval",
            RemoteWrite,
            &[57][..],
            "environment/gate IDs",
        ),
        (
            "actions-secrets-variables",
            RemoteWrite,
            &[59][..],
            "original stable resource identity",
        ),
        (
            "actions-runners",
            RemoteWrite,
            &[60][..],
            "selected membership",
        ),
    ] {
        let c = capability_requirement_contract_v1(id).unwrap();
        assert_eq!(c.effect(), effect, "{id}");
        assert_eq!(c.tasks, tasks, "{id}");
        assert!(c.target_requirement().contains(guard), "{id}");
    }
    assert!(
        capability_requirement_contract_v1("actions-secrets-variables")
            .unwrap()
            .operations
            .contains("Existing secret values cannot be read")
    );
    assert!(capability_requirement_contract_v1("actions-runners")
        .unwrap()
        .operations
        .contains("no runner service installation"));
    assert!(capability_requirement_contract_v1("pr-metadata")
        .unwrap()
        .operations
        .contains("milestone and linked issues with selected-field readback; Projects (v2) placement add/remove with item readback."));
}

#[test]
fn gh_provider_inventory_contracts_exact_cli_routes_share_only_reviewed_flag_requirements() {
    for (id, requirement) in [
        ("pr-details", CliRequirement::ApiSlurp),
        ("pr-checks", CliRequirement::RunAttempt),
        ("actions-logs", CliRequirement::RunAttempt),
        ("pr-lifecycle", CliRequirement::ReadyUndo),
        ("pr-merge", CliRequirement::MergeExpectedHead),
        (
            "actions-secrets-variables",
            CliRequirement::SecretEmptySelection,
        ),
    ] {
        let (route, actual) = capability_requirement_contract_v1(id).unwrap().cli.unwrap();
        assert_eq!(actual, requirement);
        assert!(requirement
            .exact_flags()
            .iter()
            .all(|flag| route.contains(flag)));
    }
    for id in [
        "pr-metadata",
        "pr-create",
        "actions-dispatch",
        "actions-runners",
        "actions-policy",
    ] {
        assert_eq!(capability_requirement_contract_v1(id).unwrap().cli, None);
    }
}

#[test]
fn gh_provider_inventory_contracts_legacy_flags_and_serialization_never_supply_requirement_authority(
) {
    let mut c = super::super::capability_inventory()
        .into_iter()
        .find(|c| c.id == "pr-metadata")
        .unwrap();
    let original = c.requirements_v1();
    c.implemented = !c.implemented;
    c.reason = "Claimed permission granted".into();
    c.transport = "arbitrary unreviewed transport".into();
    assert_eq!(c.requirements_v1(), original);
    let json = serde_json::to_value(&c).unwrap();
    assert!(json.get("requirements_v1").is_none());
    let parsed: Capability = serde_json::from_value(json).unwrap();
    assert_eq!(parsed.requirements_v1(), original);
    for id in ["unknown", "PR-MERGE", "actions-secrets-variables/other", ""] {
        assert!(capability_requirement_contract_v1(id).is_none());
    }
}

#[test]
fn gh_provider_inventory_contracts_text_and_evidence_are_bounded_and_explicitly_unverified() {
    for capability in super::super::capability_inventory() {
        let c = capability.requirements_v1().unwrap();
        assert!(!c.rows.is_empty() && !c.tasks.is_empty());
        assert_eq!(
            c.rows.iter().copied().collect::<BTreeSet<_>>().len(),
            c.rows.len()
        );
        assert_eq!(
            c.tasks.iter().copied().collect::<BTreeSet<_>>().len(),
            c.tasks.len()
        );
        assert!(c.tasks.iter().all(|n| (1..=79).contains(n)));
        let text = [
            c.operations,
            c.preview_requirement(),
            c.target_requirement(),
            c.paging_requirement(),
            c.host_limit(),
            c.evidence_limit(),
        ];
        assert!(text
            .iter()
            .all(|s| !s.is_empty() && !s.chars().any(char::is_control)));
        assert!(text.iter().map(|s| s.len()).sum::<usize>() < 4096);
        assert!(c.host_limit().contains("unverified here"));
        assert!(c.evidence_limit().contains("not delivery evidence"));
        assert!(c.evidence_limit().contains("disposable-repository"));
        assert!(c.evidence_limit().contains("no live probe or mutation"));
    }
}
