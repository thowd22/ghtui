//! Actual retained metadata capacities only. No decode/permission/source/claim.
use super::*;
impl ActionsWriteMarker {
    pub(crate) fn workflow_owned_bytes_v9(&self) -> Option<usize> {
        use crate::providers::gh::workflow_capacity_v9 as b;
        let c = &self.context;
        let mut n = b::sum([
            b::owner::<Self>(),
            b::strings([
                &self.target.scope.host,
                &self.target.scope.login,
                &self.fingerprint_sha256,
            ])?,
            b::actions_target(&self.target.target)?,
            b::lines(&self.target.resources)?,
            b::vector(&self.options.selected_ids)?,
            b::strings(
                [c.head_sha.as_ref(), c.workflow_commit.as_ref()]
                    .into_iter()
                    .flatten(),
            )?,
        ])?;
        if let Some(r) = &c.original_repository_v3 {
            n = n.checked_add(b::strings([&r.repository_node, &r.original_name])?)?;
        }
        if let Some(r) = &c.original_run_v3 {
            n = n.checked_add(r.run_node.capacity())?;
        }
        if let Some(pr) = &c.original_pr_v3 {
            n = n.checked_add(b::strings([&pr.node, &pr.head_sha, &pr.base_sha])?)?;
        }
        if let Some(force) = &c.force_escalation_v5 {
            n = b::sum([
                n,
                force.cancel.fingerprint_sha256.capacity(),
                b::lines(&force.cancel_resources)?,
            ])?;
        }
        if let Some(w) = &c.original_workflow_v6 {
            n = n.checked_add(b::strings([&w.node, &w.path])?)?;
        }
        if let Some(d) = &c.original_dispatch_v7 {
            n = n.checked_add(b::strings([
                &d.protocol,
                &d.namespace,
                &d.raw_name,
                &d.tip,
                &d.tip_type,
                &d.commit,
            ])?)?;
        }
        if let Some(w) = &c.workflow_edit_v9 {
            n = n.checked_add(b::strings([
                &w.head_repository_node,
                &w.head_original_name,
                &w.head_current_name,
                &w.head_ref,
                &w.base_ref,
                &w.original_commit,
                &w.committed_head,
                &w.base_commit,
            ])?)?;
        }
        if let Some(r) = &self.receipt {
            if let Some(d) = &r.original_dispatch_run_v7 {
                n = n.checked_add(b::strings([
                    &d.repository_node,
                    &d.run_node,
                    &d.head_sha,
                    &d.event,
                ])?)?;
            }
            if let Some(remote) = &r.workflow_remote_v9 {
                n = n.checked_add(match remote {
                    WorkflowRemoteReceiptV9::Branch {
                        head_repository_node,
                        head_ref,
                        committed_head,
                        ..
                    } => b::strings([head_repository_node, head_ref, committed_head])?,
                    WorkflowRemoteReceiptV9::Pr {
                        base_repository_node,
                        pr_node,
                        base_ref,
                        base_commit,
                        head_repository_node,
                        head_ref,
                        committed_head,
                        ..
                    } => b::strings([
                        base_repository_node,
                        pr_node,
                        base_ref,
                        base_commit,
                        head_repository_node,
                        head_ref,
                        committed_head,
                    ])?,
                })?;
            }
        }
        Some(n)
    }
}
