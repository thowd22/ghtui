//! Worker-only adapters/accounting over the ONE existing Actions workspace.
use super::*;
use std::mem::size_of;
const LIMIT: usize = crate::scoped_actions_v1::PREPARED_LIMIT_V1;
fn add(total: &mut usize, bytes: usize) {
    *total = total.saturating_add(bytes);
}
fn scope_bytes(scope: &ActionsScope) -> usize {
    match scope {
        ActionsScope::Repository { repository } => repository.capacity(),
        ActionsScope::Organization { organization } => organization.capacity(),
        ActionsScope::Environment {
            repository,
            environment,
        } => repository.capacity().saturating_add(environment.capacity()),
    }
}
fn strings(values: &[&String]) -> usize {
    values
        .iter()
        .map(|s| s.capacity())
        .fold(0, usize::saturating_add)
}
pub(crate) fn json_bytes(value: &Value, depth: usize) -> usize {
    if depth > 128 {
        return usize::MAX;
    }
    let head = size_of::<Value>();
    match value {
        Value::String(s) => head.saturating_add(s.capacity()),
        Value::Array(rows) => rows.iter().fold(
            head.saturating_add(rows.capacity().saturating_mul(size_of::<Value>())),
            |sum, row| sum.saturating_add(json_bytes(row, depth + 1)),
        ),
        // serde_json's opaque object allocator has no capacity getter. Charge
        // a conservative1024-byte map-node allowance PER observed member,
        // plus actual key capacities/values, not falsely introspected heap/RSS.
        Value::Object(rows) => rows.iter().fold(
            head.saturating_add(rows.len().saturating_mul(1024)),
            |sum, (key, row)| {
                sum.saturating_add(key.capacity())
                    .saturating_add(json_bytes(row, depth + 1))
            },
        ),
        _ => head,
    }
}
pub(crate) fn definition_bytes(d: &WorkflowDefinition) -> usize {
    let mut n = size_of::<WorkflowDefinition>()
        + strings(&[
            &d.workflow.name,
            &d.workflow.path,
            &d.workflow.state,
            &d.revision,
            &d.commit_sha,
            &d.yaml,
        ]);
    add(&mut n, d.triggers.capacity() * size_of::<String>());
    for row in &d.triggers {
        add(&mut n, row.capacity());
    }
    add(
        &mut n,
        d.dispatch_inputs.capacity() * size_of::<DispatchInput>(),
    );
    for row in &d.dispatch_inputs {
        add(
            &mut n,
            strings(&[&row.name, &row.description]) + row.options.capacity() * size_of::<String>(),
        );
        for option in &row.options {
            add(&mut n, option.capacity());
        }
        if let Some(value) = &row.default {
            add(&mut n, json_bytes(value, 0));
        }
    }
    add(&mut n, d.jobs.capacity() * size_of::<DefinitionJob>());
    for job in &d.jobs {
        add(
            &mut n,
            strings(&[&job.id, &job.name])
                + job.reusable_workflow.as_ref().map_or(0, String::capacity)
                + job.needs.capacity() * size_of::<String>(),
        );
        for need in &job.needs {
            add(&mut n, need.capacity());
        }
        add(&mut n, json_bytes(&job.matrix, 0));
        add(&mut n, json_bytes(&job.environment, 0));
    }
    n
}
fn secrets_bytes(rows: &Vec<SecretMetadata>) -> usize {
    rows.iter()
        .fold(rows.capacity() * size_of::<SecretMetadata>(), |sum, row| {
            sum.saturating_add(
                strings(&[&row.name, &row.created_at, &row.updated_at])
                    + row.visibility.as_ref().map_or(0, String::capacity),
            )
        })
}
fn variables_bytes(rows: &Vec<ActionsVariable>) -> usize {
    rows.iter().fold(
        rows.capacity() * size_of::<ActionsVariable>(),
        |sum, row| {
            sum.saturating_add(
                strings(&[&row.name, &row.value, &row.created_at, &row.updated_at])
                    + row.visibility.as_ref().map_or(0, String::capacity),
            )
        },
    )
}
fn runners_bytes(rows: &Vec<SelfHostedRunner>) -> usize {
    rows.iter().fold(
        rows.capacity() * size_of::<SelfHostedRunner>(),
        |sum, row| {
            sum.saturating_add(
                strings(&[&row.name, &row.os, &row.status])
                    + row.labels.capacity() * size_of::<RunnerLabel>(),
            )
            .saturating_add(
                row.labels
                    .iter()
                    .map(|label| label.name.capacity() + label.label_type.capacity())
                    .fold(0, usize::saturating_add),
            )
        },
    )
}
fn caches_bytes(rows: &Vec<ActionCache>) -> usize {
    rows.iter()
        .fold(rows.capacity() * size_of::<ActionCache>(), |sum, row| {
            sum.saturating_add(strings(&[
                &row.key,
                &row.ref_name,
                &row.version,
                &row.created_at,
                &row.last_accessed_at,
            ]))
        })
}
pub(crate) fn resource_proof_bytes(read: &ScopedActionsResourceReadV2) -> usize {
    let mut n = size_of::<ScopedActionsResourceReadV2>()
        + 2 * size_of::<usize>()
        + read.repository().owned_bytes_v2();
    match read.body() {
        ScopedActionsResourceBodyV2::Artifacts { run, targets } => {
            add(
                &mut n,
                run.owned_bytes_v1()
                    + targets.len() * size_of::<Arc<ScopedActionsArtifactV2>>()
                    + 2 * size_of::<usize>(),
            );
            for target in targets.iter() {
                add(&mut n, target.owned_bytes_v2());
            }
        }
        ScopedActionsResourceBodyV2::Artifact { target, archive } => add(
            &mut n,
            target.owned_bytes_v2()
                + target.repository().owned_bytes_v2()
                + target.run().owned_bytes_v1()
                + archive.retained_bytes_v2()
                + 2 * size_of::<usize>(),
        ),
        ScopedActionsResourceBodyV2::Workflows(targets) => {
            add(
                &mut n,
                targets.len() * size_of::<Arc<ScopedActionsWorkflowV2>>() + 2 * size_of::<usize>(),
            );
            for target in targets.iter() {
                add(&mut n, target.owned_bytes_v2());
            }
        }
        ScopedActionsResourceBodyV2::Definition { target, definition } => add(
            &mut n,
            target.owned_bytes_v2()
                + target.repository().owned_bytes_v2()
                + definition_bytes(definition)
                + 2 * size_of::<usize>(),
        ),
        ScopedActionsResourceBodyV2::Secrets(rows) => add(&mut n, secrets_bytes(rows)),
        ScopedActionsResourceBodyV2::Variables(rows) => add(&mut n, variables_bytes(rows)),
        ScopedActionsResourceBodyV2::Policy(row) => add(
            &mut n,
            size_of::<PolicySnapshot>() + scope_bytes(&row.scope) + json_bytes(&row.value, 0),
        ),
        ScopedActionsResourceBodyV2::Environments(rows) => add(
            &mut n,
            rows.capacity() * size_of::<Value>()
                + rows
                    .iter()
                    .map(|value| json_bytes(value, 0))
                    .fold(0, usize::saturating_add),
        ),
        ScopedActionsResourceBodyV2::Runners(rows) => add(&mut n, runners_bytes(rows)),
        ScopedActionsResourceBodyV2::Caches { entries, .. } => add(&mut n, caches_bytes(entries)),
        ScopedActionsResourceBodyV2::Unavailable(_) => {}
    }
    n
}
pub(crate) fn workspace_bytes(w: &ActionsWorkspace) -> usize {
    let mut n = 0usize;
    add(
        &mut n,
        w.workflows.capacity() * size_of::<Workflow>()
            + w.default_workflow_revision
                .as_ref()
                .map_or(0, String::capacity),
    );
    for row in &w.workflows {
        add(&mut n, strings(&[&row.name, &row.path, &row.state]));
    }
    if let Some(d) = &w.definition {
        add(&mut n, definition_bytes(d));
    }
    add(
        &mut n,
        w.artifacts.capacity() * size_of::<ActionsArtifact>(),
    );
    for row in &w.artifacts {
        add(&mut n, artifact_model_bytes_v2(row));
    }
    if let Some(row) = &w.artifact {
        add(&mut n, artifact_model_bytes_v2(row));
    }
    if let Some(archive) = &w.archive {
        add(&mut n, archive.retained_bytes_v2() + 2 * size_of::<usize>());
    }
    if let Some(text) = &w.artifact_preview {
        add(&mut n, text.capacity());
    }
    if let Some(image) = &w.artifact_image {
        add(
            &mut n,
            size_of::<crate::identity::ImageDisplay>()
                + 2 * size_of::<usize>()
                + image.format.capacity()
                + image.raster.pixels.capacity() * size_of::<[u8; 4]>(),
        );
    }
    add(&mut n, secrets_bytes(&w.secrets));
    add(&mut n, variables_bytes(&w.variables));
    add(&mut n, runners_bytes(&w.runners));
    add(&mut n, caches_bytes(&w.caches));
    add(
        &mut n,
        w.environments.capacity() * size_of::<Value>()
            + w.environment_rows.capacity() * size_of::<Option<u64>>(),
    );
    for value in &w.environments {
        add(&mut n, json_bytes(value, 0));
    }
    if let Some(row) = &w.policy {
        add(&mut n, scope_bytes(&row.scope) + json_bytes(&row.value, 0));
    }
    n
}
pub(crate) fn preflight(read: &ScopedActionsResourceReadV2) -> Result<(), String> {
    let mut bound = resource_proof_bytes(read)
        .saturating_mul(4)
        .saturating_add(16384);
    if let ScopedActionsResourceBodyV2::Definition { definition, .. } = read.body() {
        let lines = definition
            .yaml
            .lines()
            .count()
            .saturating_add(definition.jobs.len())
            .saturating_add(definition.dispatch_inputs.len());
        if lines > 5000 {
            return Err(
                "Workflow definition exceeds5000 display rows; no silent YAML truncation".into(),
            );
        }
        bound = bound.saturating_add(lines.saturating_mul(256));
    }
    if bound > LIMIT {
        Err("Original Actions resource allocation preflight exceeds8MiB; no model clone/rows prepared".into())
    } else {
        Ok(())
    }
}
pub(crate) fn install_data(
    w: &mut ActionsWorkspace,
    read: &ScopedActionsResourceReadV2,
    view: ActionsView,
    entry: Option<usize>,
    cancel: &CancellationToken,
) -> Result<(), String> {
    preflight(read)?;
    w.view = view;
    match read.body() {
        ScopedActionsResourceBodyV2::Artifacts { targets, .. } => {
            w.artifacts = targets
                .iter()
                .map(|target| target.model().clone())
                .collect()
        }
        ScopedActionsResourceBodyV2::Artifact { target, archive } => {
            w.artifact = Some(target.model().clone());
            w.archive = Some(archive.clone());
            if let Some(index) = entry {
                let row = archive
                    .entries
                    .get(index)
                    .ok_or("Selected original archive entry unavailable")?;
                let content = artifact::prepare_scoped_content_v2(archive, &row.path, cancel)
                    .map_err(|_| "Original bounded archive entry unavailable")?;
                w.artifact_preview = Some(content.text);
                w.artifact_image = content.image;
            }
        }
        ScopedActionsResourceBodyV2::Workflows(targets) => {
            w.default_workflow_revision = read.repository().default_branch().map(str::to_owned);
            w.workflows = targets
                .iter()
                .map(|target| target.model().clone())
                .collect();
        }
        ScopedActionsResourceBodyV2::Definition { definition, .. } => {
            w.definition = Some(definition.as_ref().clone())
        }
        ScopedActionsResourceBodyV2::Secrets(rows) => w.secrets = rows.clone(),
        ScopedActionsResourceBodyV2::Variables(rows) => w.variables = rows.clone(),
        ScopedActionsResourceBodyV2::Policy(row) => {
            w.policy_section = row.section;
            w.policy = Some(row.as_ref().clone());
        }
        ScopedActionsResourceBodyV2::Environments(rows) => w.environments = rows.clone(),
        ScopedActionsResourceBodyV2::Runners(rows) => w.runners = rows.clone(),
        ScopedActionsResourceBodyV2::Caches { entries, usage } => {
            w.caches = entries.clone();
            w.cache_usage = usage.clone();
            if usage.is_none() {
                w.warnings.push("Actual cache usage unavailable; no zero usage or administrator permission inferred".into());
            }
        }
        ScopedActionsResourceBodyV2::Unavailable(reason) => w.warnings.push((*reason).into()),
    }
    if cancel.is_cancelled() {
        return Err("Original resource preparation canceled".into());
    }
    Ok(())
}
/// Extend the same policy, controls and hit semantics only for actual bounded
/// read adapters. No Compose/publication/export/extraction gets enabled.
pub(crate) fn prepare_policy(w: &mut ActionsWorkspace) {
    use ActionsView as V;
    if w.view == V::Workflow {
        if let (Some(definition), Some(row)) = (&w.definition, w.rows.first_mut()) {
            row.label = format!(
                "Requested ref {} • resolved {}",
                definition.revision, definition.commit_sha
            );
        }
    }
    for control in &mut w.controls {
        if matches!(
            control.action,
            ActionsAction::Artifacts
                | ActionsAction::View(
                    V::Artifacts
                        | V::Workflows
                        | V::Secrets
                        | V::Variables
                        | V::Policies
                        | V::Environments
                        | V::Runners
                        | V::Caches
                )
                | ActionsAction::Policy(_)
        ) {
            control.disabled_reason = None;
        }
    }
    if w.view == V::Workflows && w.default_workflow_revision.is_none() {
        for row in &mut w.rows {
            row.action = Some(ActionsAction::Unavailable(
                "Actual default workflow revision unavailable; no HEAD or branch guessed".into(),
            ));
        }
    }
    // Resource rows carry only same-provider member IDs. Any row without a
    // current read adapter remains an explanation, never a legacy writer.
    for row in &mut w.rows {
        if row.action.as_ref().is_some_and(|action| {
            !matches!(
                action,
                ActionsAction::OpenRun { .. }
                    | ActionsAction::OpenJob(_)
                    | ActionsAction::OpenArtifact(_)
                    | ActionsAction::ArtifactEntry(_)
                    | ActionsAction::OpenWorkflow { .. }
                    | ActionsAction::Unavailable(_)
            )
        }) {
            row.action=Some(ActionsAction::Unavailable("This scoped resource action has no current read/write protocol; no fallback or publication".into()));
        }
    }
    w.controls
        .sort_by_key(|control| control.disabled_reason.is_some());
}
