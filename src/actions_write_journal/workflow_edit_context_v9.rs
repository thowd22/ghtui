//! Descriptive W9 remote metadata ONLY. No saved bytes mint a live source,
//! target/permission, retry, original guard, claim or publisher capability.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowRemotePhaseV9 {
    Branch,
    Pr,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionsOriginalWorkflowEditContextV9 {
    pub phase: WorkflowRemotePhaseV9,
    pub head_repository_id: u64,
    pub head_repository_node: String,
    pub head_original_name: String,
    pub head_current_name: String,
    pub head_ref: String,
    pub base_ref: String,
    pub original_commit: String,
    pub committed_head: String,
    pub base_commit: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkflowRemoteReceiptV9 {
    Branch {
        head_repository_id: u64,
        head_repository_node: String,
        head_ref: String,
        committed_head: String,
    },
    Pr {
        base_repository_id: u64,
        base_repository_node: String,
        pr_id: u64,
        pr_node: String,
        number: u64,
        base_ref: String,
        base_commit: String,
        head_repository_id: u64,
        head_repository_node: String,
        head_ref: String,
        committed_head: String,
    },
}
fn positive(n: u64) -> bool {
    (1..=i64::MAX as u64).contains(&n)
}
fn node(s: &str) -> bool {
    !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control)
}
fn sha(s: &str) -> bool {
    matches!(s.len(), 40 | 64) && s.bytes().all(|c| c.is_ascii_hexdigit())
}
fn branch(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 256
        && !s.starts_with('-')
        && !s.starts_with("refs/")
        && !s.ends_with('.')
        && !s.contains("..")
        && !s.contains("@{")
        && s != "@"
        && !s.chars().any(|c| {
            c.is_control()
                || c.is_whitespace()
                || matches!(c, '~' | '^' | ':' | '?' | '*' | '[' | '\\')
        })
        && s.split('/')
            .all(|p| !p.is_empty() && !p.starts_with('.') && !p.ends_with(".lock"))
}
pub(super) fn validate_context(context: &ActionsImmutableContext) -> Result<()> {
    let Some(edit) = &context.workflow_edit_v9 else {
        return Ok(());
    };
    if context.original_repository_v3.is_none()
        || context.original_workflow_v6.is_none()
        || context.original_run_v3.is_some()
        || context.original_dispatch_v7.is_some()
        || context.force_escalation_v5.is_some()
        || context.run_id.is_some()
        || context.attempt.is_some()
        || context.head_sha.is_some()
        || context.job_id.is_some()
        || !positive(edit.head_repository_id)
        || !node(&edit.head_repository_node)
        || !branch(&edit.head_ref)
        || !branch(&edit.base_ref)
        || !sha(&edit.original_commit)
        || !sha(&edit.committed_head)
        || !sha(&edit.base_commit)
        || edit.original_commit == edit.committed_head
        || context.workflow_commit.as_deref() != Some(edit.original_commit.as_str())
    {
        bail!("Original Workflow remote metadata has no exact bounded repository/ref/C-D identity");
    }
    for name in [&edit.head_original_name, &edit.head_current_name] {
        if name.len() > 256 {
            bail!("Original head repository name exceeds metadata bound");
        }
        crate::config::validate_repository(name)?;
    }
    Ok(())
}
pub(super) fn validate_intent(intent: &ActionsWriteIntent) -> Result<()> {
    let Some(edit) = &intent.context.workflow_edit_v9 else {
        if matches!(
            intent.kind,
            ActionsWriteKind::PublishWorkflowBranch | ActionsWriteKind::CreateWorkflowPr
        ) {
            bail!("Workflow remote kind requires exact original scoped metadata");
        }
        return Ok(());
    };
    let kind = match edit.phase {
        WorkflowRemotePhaseV9::Branch => ActionsWriteKind::PublishWorkflowBranch,
        WorkflowRemotePhaseV9::Pr => ActionsWriteKind::CreateWorkflowPr,
    };
    if intent.kind != kind
        || intent.options.debug_logging
        || intent.options.enabled.is_some()
        || !intent.options.selected_ids.is_empty()
    {
        bail!("Original Workflow remote metadata differs from its explicit effect");
    }
    Ok(())
}
pub(super) fn extend_aliases(
    host: &str,
    context: &ActionsImmutableContext,
    out: &mut Vec<String>,
) -> Result<()> {
    let Some(edit) = &context.workflow_edit_v9 else {
        return Ok(());
    };
    validate_context(context)?;
    // BOTH actual original and observed current head names are persisted.
    // Never guess an old alias from a current target after rename/restart.
    for name in [&edit.head_original_name, &edit.head_current_name] {
        out.push(format!(
            "{}/{}/branch/{}",
            host,
            name.to_ascii_lowercase(),
            edit.head_ref
        ));
    }
    out.push(format!(
        "{host}/repository-id/{}/git/branch/{}",
        edit.head_repository_id, edit.head_ref
    ));
    Ok(())
}
pub(super) fn validate_receipt(receipt: &ActionsReceipt) -> Result<()> {
    let Some(remote) = &receipt.workflow_remote_v9 else {
        if matches!(
            receipt.kind,
            ActionsReceiptKind::WorkflowBranchObserved | ActionsReceiptKind::WorkflowPrObserved
        ) {
            bail!("Workflow remote receipt requires actual typed facts");
        }
        return Ok(());
    };
    let (id, n, r, d, kind) = match remote {
        WorkflowRemoteReceiptV9::Branch {
            head_repository_id,
            head_repository_node,
            head_ref,
            committed_head,
        } => (
            *head_repository_id,
            head_repository_node,
            head_ref,
            committed_head,
            ActionsReceiptKind::WorkflowBranchObserved,
        ),
        WorkflowRemoteReceiptV9::Pr {
            base_repository_id,
            base_repository_node,
            pr_id,
            pr_node,
            number,
            base_ref,
            base_commit,
            head_repository_id,
            head_repository_node,
            head_ref,
            committed_head,
        } => {
            if !positive(*base_repository_id)
                || !node(base_repository_node)
                || !positive(*pr_id)
                || !node(pr_node)
                || !positive(*number)
                || !branch(base_ref)
                || !sha(base_commit)
            {
                bail!("Workflow PR receipt omitted actual positive canonical facts");
            }
            (
                *head_repository_id,
                head_repository_node,
                head_ref,
                committed_head,
                ActionsReceiptKind::WorkflowPrObserved,
            )
        }
    };
    if receipt.kind != kind
        || receipt.resource_id
            != Some(match remote {
                WorkflowRemoteReceiptV9::Branch {
                    head_repository_id, ..
                } => *head_repository_id,
                WorkflowRemoteReceiptV9::Pr { number, .. } => *number,
            })
        || !positive(id)
        || !node(n)
        || !branch(r)
        || !sha(d)
        || receipt.observed_attempt.is_some()
        || receipt.observed_run_id.is_some()
        || receipt.original_dispatch_run_v7.is_some()
    {
        bail!("Workflow remote receipt cannot impersonate another effect or run");
    }
    Ok(())
}
pub(super) fn validate_receipt_target(
    marker: &ActionsWriteMarker,
    receipt: &ActionsReceipt,
) -> Result<()> {
    match (
        &marker.context.workflow_edit_v9,
        &receipt.workflow_remote_v9,
    ) {
        (None, None) => Ok(()),
        (Some(edit), Some(remote)) => {
            let base = marker
                .context
                .original_repository_v3
                .as_ref()
                .context("Original Workflow remote base absent")?;
            let (id, n, r, d) = match (edit.phase, remote) {
                (
                    WorkflowRemotePhaseV9::Branch,
                    WorkflowRemoteReceiptV9::Branch {
                        head_repository_id,
                        head_repository_node,
                        head_ref,
                        committed_head,
                    },
                ) => (
                    *head_repository_id,
                    head_repository_node,
                    head_ref,
                    committed_head,
                ),
                (
                    WorkflowRemotePhaseV9::Pr,
                    WorkflowRemoteReceiptV9::Pr {
                        base_repository_id,
                        base_repository_node,
                        base_ref,
                        base_commit,
                        head_repository_id,
                        head_repository_node,
                        head_ref,
                        committed_head,
                        ..
                    },
                ) => {
                    if *base_repository_id != base.repository_id
                        || base_repository_node != &base.repository_node
                        || base_ref != &edit.base_ref
                        || base_commit != &edit.base_commit
                    {
                        bail!("Workflow PR receipt differs from exact original base");
                    }
                    (
                        *head_repository_id,
                        head_repository_node,
                        head_ref,
                        committed_head,
                    )
                }
                _ => bail!("Workflow remote receipt has a different effect kind"),
            };
            if id != edit.head_repository_id
                || n != &edit.head_repository_node
                || r != &edit.head_ref
                || d != &edit.committed_head
            {
                bail!("Workflow remote receipt differs from exact original head");
            }
            Ok(())
        }
        _ => bail!("Workflow remote context and typed receipt must remain paired"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn context(phase: WorkflowRemotePhaseV9) -> ActionsImmutableContext {
        ActionsImmutableContext {
            workflow_commit: Some("a".repeat(40)),
            original_repository_v3: Some(ActionsOriginalRepositoryContextV3 {
                repository_id: 10,
                repository_node: "BASE10".into(),
                original_name: "old/base".into(),
            }),
            original_workflow_v6: Some(ActionsOriginalWorkflowContextV6 {
                workflow_id: 7,
                node: "WF7".into(),
                path: ".github/workflows/ci.yml".into(),
            }),
            workflow_edit_v9: Some(ActionsOriginalWorkflowEditContextV9 {
                phase,
                head_repository_id: 20,
                head_repository_node: "HEAD20".into(),
                head_original_name: "old/head".into(),
                head_current_name: "new/head".into(),
                head_ref: "workflow/Fix".into(),
                base_ref: "main".into(),
                original_commit: "a".repeat(40),
                committed_head: "d".repeat(40),
                base_commit: "b".repeat(40),
            }),
            ..Default::default()
        }
    }
    fn target(context: &ActionsImmutableContext) -> ActionsWriteTarget {
        ActionsWriteTarget::from_original_context_v3(
            &Scope::github("github.invalid", Some(42)).unwrap(),
            &ActionsTarget::Workflow {
                repository: "new/base".into(),
                id: 7,
            },
            context,
        )
        .unwrap()
    }
    #[test]
    fn task54_workflow_edit_v9_original_and_current_head_aliases_reconstruct_purely_without_names_guess(
    ) {
        let context = context(WorkflowRemotePhaseV9::Branch);
        let actual_target = target(&context);
        assert_eq!(
            actual_target.resource_keys(),
            [
                "github.invalid/new/base/workflow/7",
                "github.invalid/new/head/branch/workflow/Fix",
                "github.invalid/old/base/workflow/7",
                "github.invalid/old/head/branch/workflow/Fix",
                "github.invalid/repository-id/10/actions/workflow/7",
                "github.invalid/repository-id/20/git/branch/workflow/Fix",
            ]
        );
        let body = serde_json::to_vec(&context).unwrap();
        let restored: ActionsImmutableContext = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            actual_target.resource_keys(),
            target(&restored).resource_keys()
        );
    }
    #[test]
    fn task54_workflow_edit_v9_new_optional_metadata_keeps_exact_legacy_none_bytes() {
        let context = ActionsImmutableContext::default();
        assert_eq!(
            serde_json::to_string(&context).unwrap(),
            r#"{"run_id":null,"attempt":null,"head_sha":null,"workflow_commit":null,"job_id":null}"#
        );
        let receipt = ActionsReceipt {
            kind: ActionsReceiptKind::RunTransition,
            resource_id: Some(123),
            observed_attempt: Some(2),
            observed_run_id: None,
            original_dispatch_run_v7: None,
            workflow_remote_v9: None,
        };
        assert_eq!(
            serde_json::to_string(&receipt).unwrap(),
            r#"{"kind":"run_transition","resource_id":123,"observed_attempt":2,"observed_run_id":null}"#
        );
        let prior:ActionsImmutableContext=serde_json::from_str(r#"{"run_id":null,"attempt":null,"head_sha":null,"workflow_commit":null,"job_id":null}"#).unwrap();
        assert_eq!(prior, context);
    }
    #[test]
    fn task54_workflow_edit_v9_branch_pr_share_real_immediate_protection_and_do_not_persist_private_payload(
    ) {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::Builder::new()
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let mut journal = ActionsWriteJournal::open(&dir.path().join("cache.sqlite3")).unwrap();
        let branch = context(WorkflowRemotePhaseV9::Branch);
        let branch_target = target(&branch);
        let branch_intent = ActionsWriteIntent::new(
            ActionsWriteKind::PublishWorkflowBranch,
            branch,
            Default::default(),
            b"PRIVATE_YAML_TITLE_BODY_NOT_METADATA",
        )
        .unwrap();
        let permit = journal.begin(&branch_target, &branch_intent).unwrap();
        journal
            .claim(&permit, &branch_target, &branch_intent)
            .unwrap();
        journal.mark_uncertain(&permit).unwrap();
        let pr = context(WorkflowRemotePhaseV9::Pr);
        let pr_target = target(&pr);
        let pr_intent = ActionsWriteIntent::new(
            ActionsWriteKind::CreateWorkflowPr,
            pr,
            Default::default(),
            b"PRIVATE_YAML_TITLE_BODY_NOT_METADATA",
        )
        .unwrap();
        let before = journal.connection.total_changes();
        assert!(journal.begin(&pr_target, &pr_intent).is_err());
        assert_eq!(journal.connection.total_changes(), before);
        let page = journal.list_unresolved(branch_target.scope(), 256).unwrap();
        assert_eq!(page.markers.len(), 1);
        assert_eq!(page.markers[0].state, ActionsWriteState::Uncertain);
        assert!(page.markers[0].blocks_target(&pr_target, ActionsWriteKind::CreateWorkflowPr));
        let metadata: String = journal
            .connection
            .query_row(
                "SELECT context_json FROM actions_write_attempts WHERE attempt_id=?1",
                [permit.marker().attempt_id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(!metadata.contains("PRIVATE_YAML_TITLE_BODY_NOT_METADATA"));
        page.markers[0].validate_metadata().unwrap();
    }
    #[test]
    fn task54_workflow_edit_v9_receipt_kind_and_full_head_base_pins_never_borrow_dispatch_or_run_semantics(
    ) {
        let context = context(WorkflowRemotePhaseV9::Branch);
        let target = target(&context);
        let intent = ActionsWriteIntent::new(
            ActionsWriteKind::PublishWorkflowBranch,
            context,
            Default::default(),
            b"",
        )
        .unwrap();
        let receipt = ActionsReceipt {
            kind: ActionsReceiptKind::WorkflowBranchObserved,
            resource_id: Some(20),
            observed_attempt: None,
            observed_run_id: None,
            original_dispatch_run_v7: None,
            workflow_remote_v9: Some(WorkflowRemoteReceiptV9::Branch {
                head_repository_id: 20,
                head_repository_node: "HEAD20".into(),
                head_ref: "workflow/Fix".into(),
                committed_head: "d".repeat(40),
            }),
        };
        validate_receipt(&receipt).unwrap();
        let marker = ActionsWriteMarker {
            attempt_id: 1,
            target: target.clone(),
            kind: intent.kind,
            context: intent.context.clone(),
            options: intent.options.clone(),
            fingerprint_sha256: intent.fingerprint(&target).unwrap(),
            state: ActionsWriteState::Confirmed,
            receipt: Some(receipt.clone()),
            created_at: 1,
        };
        marker.validate_metadata().unwrap();
        for mode in 0..4 {
            let mut wrong = receipt.clone();
            match mode {
                0 => wrong.observed_run_id = Some(123),
                1 => wrong.kind = ActionsReceiptKind::Dispatch,
                3 => wrong.resource_id = Some(7),
                _ => {
                    if let Some(WorkflowRemoteReceiptV9::Branch { committed_head, .. }) =
                        &mut wrong.workflow_remote_v9
                    {
                        *committed_head = "e".repeat(40);
                    }
                }
            };
            let mut drift = marker.clone();
            drift.receipt = Some(wrong);
            assert!(drift.validate_metadata().is_err());
        }
    }
    fn actual_effect_receipt(phase: WorkflowRemotePhaseV9) -> ActionsReceipt {
        ActionsReceipt {
            kind: if phase == WorkflowRemotePhaseV9::Branch {
                ActionsReceiptKind::WorkflowBranchObserved
            } else {
                ActionsReceiptKind::WorkflowPrObserved
            },
            resource_id: Some(if phase == WorkflowRemotePhaseV9::Branch {
                20
            } else {
                91
            }),
            observed_attempt: None,
            observed_run_id: None,
            original_dispatch_run_v7: None,
            workflow_remote_v9: Some(if phase == WorkflowRemotePhaseV9::Branch {
                WorkflowRemoteReceiptV9::Branch {
                    head_repository_id: 20,
                    head_repository_node: "HEAD20".into(),
                    head_ref: "workflow/Fix".into(),
                    committed_head: "d".repeat(40),
                }
            } else {
                WorkflowRemoteReceiptV9::Pr {
                    base_repository_id: 10,
                    base_repository_node: "BASE10".into(),
                    pr_id: 9901,
                    pr_node: "PR9901".into(),
                    number: 91,
                    base_ref: "main".into(),
                    base_commit: "b".repeat(40),
                    head_repository_id: 20,
                    head_repository_node: "HEAD20".into(),
                    head_ref: "workflow/Fix".into(),
                    committed_head: "d".repeat(40),
                }
            }),
        }
    }
    fn actual_journal_effect_cycle(phase: WorkflowRemotePhaseV9) {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::Builder::new()
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let mut journal =
            ActionsWriteJournal::open(&directory.path().join("cache.sqlite3")).unwrap();
        let context = context(phase);
        let target = target(&context);
        let intent = ActionsWriteIntent::new(
            if phase == WorkflowRemotePhaseV9::Branch {
                ActionsWriteKind::PublishWorkflowBranch
            } else {
                ActionsWriteKind::CreateWorkflowPr
            },
            context,
            Default::default(),
            b"private never persisted",
        )
        .unwrap();
        let permit = journal.begin(&target, &intent).unwrap();
        journal.claim(&permit, &target, &intent).unwrap();
        let receipt = actual_effect_receipt(phase);
        let old = read_attempt(
            &journal.connection,
            target.scope(),
            permit.marker().attempt_id,
        )
        .unwrap()
        .unwrap();
        let generation = generations(&journal.connection, &target).unwrap();
        for resource_id in [None, Some(7), Some(999)] {
            let mut wrong = receipt.clone();
            wrong.resource_id = resource_id;
            let changes = journal.connection.total_changes();
            assert!(journal.mark_confirmed(&permit, &wrong).is_err());
            assert_eq!(
                journal.connection.total_changes(),
                changes,
                "Wrong effect ID refuses before ANY transition"
            );
            assert_eq!(
                read_attempt(&journal.connection, target.scope(), old.attempt_id)
                    .unwrap()
                    .unwrap()
                    .state,
                ActionsWriteState::Pending
            );
            assert_eq!(
                generations(&journal.connection, &target).unwrap(),
                generation
            );
        }
        for wrong in [
            ActionsReceipt {
                kind: ActionsReceiptKind::Dispatch,
                ..receipt.clone()
            },
            ActionsReceipt {
                workflow_remote_v9: None,
                ..receipt.clone()
            },
        ] {
            let changes = journal.connection.total_changes();
            assert!(journal.mark_confirmed(&permit, &wrong).is_err());
            assert_eq!(journal.connection.total_changes(), changes);
        }
        journal.mark_confirmed(&permit, &receipt).unwrap();
        let marker = read_attempt(&journal.connection, target.scope(), old.attempt_id)
            .unwrap()
            .unwrap();
        assert_eq!(marker.state, ActionsWriteState::Confirmed);
        assert_eq!(marker.receipt.as_ref(), Some(&receipt));
        marker.validate_metadata().unwrap();
        assert_eq!(
            generations(&journal.connection, &target).unwrap(),
            generation.iter().map(|g| g + 1).collect::<Vec<_>>()
        );
    }
    #[test]
    fn task54_workflow_edit_v9_actual_branch_head_repository_receipt_finishes_same_journal_not_workflow_id(
    ) {
        actual_journal_effect_cycle(WorkflowRemotePhaseV9::Branch)
    }
    #[test]
    fn task54_workflow_edit_v9_actual_pr_number_receipt_finishes_same_journal_not_workflow_id() {
        actual_journal_effect_cycle(WorkflowRemotePhaseV9::Pr)
    }
    #[test]
    fn task54_workflow_edit_v9_none_workflow_receipt_keeps_legacy_id_and_serialized_bytes() {
        let context = ActionsImmutableContext::default();
        let target = target(&context);
        let intent = ActionsWriteIntent::new(
            ActionsWriteKind::DisableWorkflow,
            context,
            ActionsSafeOptions {
                enabled: Some(false),
                ..Default::default()
            },
            b"",
        )
        .unwrap();
        let receipt = ActionsReceipt {
            kind: ActionsReceiptKind::WorkflowState,
            resource_id: Some(7),
            observed_attempt: None,
            observed_run_id: None,
            original_dispatch_run_v7: None,
            workflow_remote_v9: None,
        };
        let marker = ActionsWriteMarker {
            attempt_id: 1,
            target: target.clone(),
            kind: intent.kind,
            context: intent.context.clone(),
            options: intent.options.clone(),
            fingerprint_sha256: intent.fingerprint(&target).unwrap(),
            state: ActionsWriteState::Confirmed,
            receipt: Some(receipt.clone()),
            created_at: 1,
        };
        marker.validate_metadata().unwrap();
        assert_eq!(
            serde_json::to_string(&receipt).unwrap(),
            r#"{"kind":"workflow_state","resource_id":7,"observed_attempt":null,"observed_run_id":null}"#
        );
        let mut wrong = marker;
        wrong.receipt.as_mut().unwrap().resource_id = Some(20);
        assert!(wrong.validate_metadata().is_err());
    }
}
