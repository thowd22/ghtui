//! Closed durable metadata only, NEVER current source/auth/write authority.
//! All-None legacy bytes/paths remain intact. Only explicit scoped attempts use
//! the conservative same-account UNKNOWN-legacy check inside begin's transaction.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionsOriginalRepositoryContextV3 {
    pub repository_id: u64,
    pub repository_node: String,
    pub original_name: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionsOriginalRunContextV3 {
    pub run_node: String,
    pub workflow_id: u64,
    pub check_suite_id: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionsOriginalPrContextV3 {
    pub id: u64,
    pub node: String,
    pub number: u64,
    pub head_sha: String,
    pub base_sha: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionsOriginalWorkflowContextV6 {
    pub workflow_id: u64,
    pub node: String,
    pub path: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionsOriginalDispatchContextV7 {
    pub protocol: String,
    pub namespace: String,
    pub raw_name: String,
    pub tip: String,
    pub tip_type: String,
    pub commit: String,
}
fn positive(id: u64) -> bool {
    id > 0 && id <= i64::MAX as u64
}
fn node(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
fn sha(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
pub(super) fn validate(context: &ActionsImmutableContext) -> Result<()> {
    super::force_escalation_v5::validate_metadata(context)?;
    super::workflow_edit_context_v9::validate_context(context)?;
    let Some(repository) = &context.original_repository_v3 else {
        if context.original_run_v3.is_some()
            || context.original_pr_v3.is_some()
            || context.original_workflow_v6.is_some()
            || context.original_dispatch_v7.is_some()
            || context.workflow_edit_v9.is_some()
        {
            bail!("Original run/PR metadata requires its original numeric repository");
        }
        return Ok(());
    };
    if !positive(repository.repository_id)
        || !node(&repository.repository_node)
        || repository.original_name.len() > 256
        || repository.original_name.chars().any(char::is_control)
    {
        bail!(
            "Original repository metadata must contain bounded actual identities and observed name"
        );
    }
    // This existing pure canonicalizer applies the ONE existing repo grammar.
    // The constant host/id is grammar input, not a source, issuer or API call.
    crate::providers::gh::canonical_actions_resource_keys(
        "github.invalid",
        &ActionsTarget::Run {
            repository: repository.original_name.clone(),
            id: 1,
        },
    )
    .map_err(anyhow::Error::msg)?;
    if let Some(run) = &context.original_run_v3 {
        if !node(&run.run_node)
            || !positive(run.workflow_id)
            || run.check_suite_id.is_some_and(|id| !positive(id))
            || context.run_id.is_none()
            || context.attempt.is_none()
            || context.head_sha.is_none()
        {
            bail!("Original run metadata requires the actual run node/workflow/attempt/head");
        }
    }
    if let Some(workflow) = &context.original_workflow_v6 {
        if !positive(workflow.workflow_id)
            || !node(&workflow.node)
            || workflow.path.len() > 4096
            || workflow.path.chars().any(char::is_control)
            || !workflow.path.starts_with(".github/workflows/")
            || workflow
                .path
                .split('/')
                .any(|part| matches!(part, "" | "." | ".."))
            || context.original_run_v3.is_some()
            || context.run_id.is_some()
            || context.attempt.is_some()
            || context.head_sha.is_some()
            || context.job_id.is_some()
            || context.force_escalation_v5.is_some()
        {
            bail!(
                "Original workflow metadata requires exact node/path and no invented run authority"
            );
        }
    }
    if let Some(pr) = &context.original_pr_v3 {
        if !positive(pr.id)
            || !positive(pr.number)
            || !node(&pr.node)
            || !sha(&pr.head_sha)
            || !sha(&pr.base_sha)
        {
            bail!("Original PR metadata requires distinct actual bounded PR/head/base facts");
        }
    }
    if let Some(dispatch) = &context.original_dispatch_v7 {
        let kind = match dispatch.namespace.as_str() {
            "branch" => crate::providers::gh::RemoteReferenceKindV1::Branch,
            "tag" => crate::providers::gh::RemoteReferenceKindV1::Tag,
            _ => bail!("Historical dispatch ref namespace invalid"),
        };
        if context.original_workflow_v6.is_none()
            || context.workflow_commit.as_deref() != Some(dispatch.commit.as_str())
            || !matches!(dispatch.protocol.as_str(), "2022-11-28" | "2026-03-10")
            || !sha(&dispatch.commit)
            || !sha(&dispatch.tip)
            || !matches!(dispatch.tip_type.as_str(), "commit" | "tag")
            || dispatch.namespace == "branch" && dispatch.tip_type != "commit"
            || crate::providers::gh::DispatchRefChoiceV7::new(kind, &dispatch.raw_name).is_err()
        {
            bail!("Historical dispatch metadata is invalid or contains no exact workflow/ref/version facts");
        }
    }
    Ok(())
}

impl ActionsWriteTarget {
    /// Pure durable-key reconstruction, not an actual source/permission factory.
    /// Public metadata can never satisfy the later closed provider boundary.
    pub(crate) fn from_original_context_v3(
        scope: &Scope,
        target: &ActionsTarget,
        context: &ActionsImmutableContext,
    ) -> Result<Self> {
        validate_context(context)?;
        let mut base = Self::from_scope(scope, target)?;
        let Some(original) = &context.original_repository_v3 else {
            return Ok(base);
        };
        let (id, old_target, namespace) = match target {
            ActionsTarget::Run { id, .. }
                if context.run_id == Some(*id)
                    && context.original_run_v3.is_some()
                    && context.original_workflow_v6.is_none() =>
            {
                (
                    *id,
                    ActionsTarget::Run {
                        repository: original.original_name.clone(),
                        id: *id,
                    },
                    "run",
                )
            }
            ActionsTarget::Workflow { id, .. }
                if context
                    .original_workflow_v6
                    .as_ref()
                    .is_some_and(|workflow| workflow.workflow_id == *id)
                    && context.original_run_v3.is_none() =>
            {
                (
                    *id,
                    ActionsTarget::Workflow {
                        repository: original.original_name.clone(),
                        id: *id,
                    },
                    "workflow",
                )
            }
            _ => bail!("Original numeric Actions context has no reviewed exact target adapter"),
        };
        base.resources.extend(
            crate::providers::gh::canonical_actions_resource_keys(&scope.host, &old_target)
                .map_err(anyhow::Error::msg)?,
        );
        base.resources.push(format!(
            "{}/repository-id/{}/actions/{namespace}/{id}",
            scope.host, original.repository_id
        ));
        if let Some(escalation) = &context.force_escalation_v5 {
            super::force_escalation_v5::validate_aliases(&scope.host, id, escalation)?;
            base.resources
                .extend(escalation.cancel_resources.iter().cloned());
        }
        super::workflow_edit_context_v9::extend_aliases(&scope.host, context, &mut base.resources)?;
        base.resources.sort();
        base.resources.dedup();
        if base.resources.len() > 16 || base.resources.iter().any(|key| key.len() > 4096) {
            bail!("Original Actions aliases exceed their bounded canonical namespace");
        }
        Ok(base)
    }
}

/// SAME immediate transaction as begin(), before generation/attempt writes.
/// A bounded partial scope is never enough to infer no UNKNOWN legacy barrier.
pub(super) fn before_begin(
    transaction: &Transaction<'_>,
    target: &ActionsWriteTarget,
    intent: &ActionsWriteIntent,
) -> Result<()> {
    if intent.context.original_repository_v3.is_none() {
        return Ok(());
    }
    let scope = &target.scope;
    let provider = availability::validate_scope(scope)?;
    let ids = {
        let mut statement = transaction.prepare("SELECT attempt_id,length(CAST(target_json AS BLOB))+length(CAST(resources_json AS BLOB))+length(CAST(kind_json AS BLOB))+length(CAST(context_json AS BLOB))+length(CAST(options_json AS BLOB))+length(CAST(fingerprint_sha256 AS BLOB))+COALESCE(length(CAST(receipt_json AS BLOB)),0) FROM actions_write_attempts WHERE host=?1 AND principal=?2 AND state IN('pending','uncertain','accepted_pending') ORDER BY attempt_id DESC LIMIT 257")?;
        let rows = statement.query_map(params![scope.host, scope.login], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    if ids.len() > 256 {
        bail!("Original Actions scope exceeds256 unresolved candidates; UNKNOWN legacy coverage is incomplete, no new attempt authorized");
    }
    let mut total = 0usize;
    for (id, bytes) in &ids {
        if *id <= 0 || !(1..=128 * 1024).contains(bytes) {
            bail!("Original Actions scope metadata exceeds128KiB per candidate; no UNKNOWN absence inferred");
        }
        total = total.checked_add(*bytes as usize).filter(|bytes| *bytes <= 8*1024*1024)
            .context("Original Actions scope metadata exceeds8MiB; UNKNOWN legacy coverage is incomplete")?;
    }
    // At most one already-preflighted candidate's JSON is resident here.
    // Valid numeric provenance from an unrelated repository is not UNKNOWN.
    for (id, _) in ids {
        let marker = read_attempt(transaction, scope, id)?
            .context("Original Actions candidate changed during its coherent snapshot")?;
        availability::validate_saved_marker(&provider, &marker)?;
        if marker.context.original_repository_v3.is_none() {
            bail!("An unresolved original Actions marker has UNKNOWN legacy repository identity; inspect its original result, no numeric provenance or nonoverlap inferred");
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "scoped_context_v3_tests.rs"]
mod tests;
