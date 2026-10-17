//! Checked borrowed allocation accounting ONLY. No serialization, clone, I/O,
//! source/permission constructor or provider exposure. Existing owners expose
//! their actual private capacities through equally closed accounting methods.
use super::*;
pub(crate) fn sum(values: impl IntoIterator<Item = usize>) -> Option<usize> {
    values.into_iter().try_fold(0usize, usize::checked_add)
}
pub(crate) fn strings<'a>(values: impl IntoIterator<Item = &'a String>) -> Option<usize> {
    sum(values.into_iter().map(String::capacity))
}
pub(crate) fn vector<T>(values: &Vec<T>) -> Option<usize> {
    values.capacity().checked_mul(std::mem::size_of::<T>())
}
pub(crate) fn lines(values: &Vec<String>) -> Option<usize> {
    vector(values)?.checked_add(strings(values)?)
}
pub(crate) fn owner<T>() -> usize {
    std::mem::size_of::<T>() + 2 * std::mem::size_of::<usize>()
}
pub(crate) fn identity(value: &HostIdentity) -> Option<usize> {
    strings([&value.host, &value.login, &value.gh_version])
}
pub(crate) fn actor(value: &Actor) -> Option<usize> {
    strings(
        [&value.login, &value.actor_type].into_iter().chain(
            [value.display_name.as_ref(), value.avatar_url.as_ref()]
                .into_iter()
                .flatten(),
        ),
    )
}
pub(crate) fn summary(value: &PrSummary) -> Option<usize> {
    let p = &value.pr;
    strings(
        [
            &p.host,
            &p.id,
            &p.repository,
            &p.title,
            &p.body,
            &p.url,
            &p.state,
            &p.created_at,
            &p.updated_at,
            &value.head_sha,
            &value.base_sha,
            &value.head_ref,
            &value.base_ref,
            &value.mergeable_state,
        ]
        .into_iter()
        .chain(value.merge_commit_sha.iter()),
    )?
    .checked_add(actor(&p.author)?)?
    // Pinned B6 set: 11 scalar Queue slots,12 edges/header/alignment fit
    // 512 bytes. At most2n+1 nodes dominates sparse/split roots, including
    // a possible retained empty root. Never infer capacity from len alone.
    .checked_add(
        p.queues
            .len()
            .checked_mul(2)?
            .checked_add(1)?
            .checked_mul(512)?,
    )
}
pub(crate) fn check(value: &Check) -> Option<usize> {
    strings([
        &value.name,
        &value.status,
        &value.conclusion,
        &value.url,
        &value.head_sha,
        &value.kind,
    ])
}
pub(crate) fn actor_identity(value: &crate::identity::ActorIdentity) -> Option<usize> {
    strings(
        [
            Some(&value.name),
            value.host.as_ref(),
            value.login.as_ref(),
            value.email.as_ref(),
            value.avatar_url.as_ref(),
        ]
        .into_iter()
        .flatten(),
    )
}
pub(crate) fn diff(value: &crate::diff::Diff) -> Option<usize> {
    let mut n = sum([
        owner::<crate::diff::Diff>(),
        strings(
            [value.base.as_ref(), value.head.as_ref()]
                .into_iter()
                .flatten(),
        )?,
        vector(&value.files)?,
    ])?;
    for file in &value.files {
        n = sum([
            n,
            file.old_path.as_ref().map_or(0, Vec::capacity),
            file.new_path.as_ref().map_or(0, Vec::capacity),
            vector(&file.hunks)?,
            vector(&file.metadata)?,
        ])?;
        n = n.checked_add(sum(file.metadata.iter().map(Vec::capacity))?)?;
        for hunk in &file.hunks {
            n = sum([
                n,
                hunk.heading.capacity(),
                vector(&hunk.lines)?,
                sum(hunk.lines.iter().map(|line| line.text.capacity()))?,
            ])?;
        }
    }
    Some(n)
}
pub(crate) fn actions_scope(value: &ActionsScope) -> Option<usize> {
    match value {
        ActionsScope::Repository { repository } => Some(repository.capacity()),
        ActionsScope::Environment {
            repository,
            environment,
        } => strings([repository, environment]),
        ActionsScope::Organization { organization } => Some(organization.capacity()),
    }
}
pub(crate) fn actions_target(value: &ActionsTarget) -> Option<usize> {
    match value {
        ActionsTarget::Run { repository, .. }
        | ActionsTarget::Workflow { repository, .. }
        | ActionsTarget::Artifact { repository, .. }
        | ActionsTarget::Cache { repository, .. } => Some(repository.capacity()),
        ActionsTarget::DeploymentReview {
            repository,
            head_sha,
            environment_ids,
            ..
        } => strings([repository, head_sha])?.checked_add(vector(environment_ids)?),
        ActionsTarget::Policy { scope, .. } | ActionsTarget::Runner { scope, .. } => {
            actions_scope(scope)
        }
        ActionsTarget::Environment { repository, name }
        | ActionsTarget::EnvironmentProtection {
            repository, name, ..
        } => strings([repository, name]),
        ActionsTarget::RunnerGroup { organization, .. }
        | ActionsTarget::GroupAssociations { organization, .. } => Some(organization.capacity()),
        ActionsTarget::RunnerGroupCreation { organization, name } => strings([organization, name]),
        ActionsTarget::DeploymentPatterns {
            repository,
            environment,
        }
        | ActionsTarget::CustomProtection {
            repository,
            environment,
        } => strings([repository, environment]),
        ActionsTarget::Secret { scope, name }
        | ActionsTarget::Variable { scope, name }
        | ActionsTarget::SettingMemberships { scope, name, .. } => {
            actions_scope(scope)?.checked_add(name.capacity())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn task54_workflow_edit_v9_graph_capacity_counts_actual_spare_vectors_and_bytes_not_only_text_lengths(
    ) {
        let mut diff = crate::diff::Diff::parse(
            b"diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-old\n+new\n",
        )
        .unwrap();
        let text_before = format!("{diff:?}");
        let before = super::diff(&diff).unwrap();
        diff.files.reserve_exact(32);
        diff.files[0].hunks.reserve_exact(16);
        diff.files[0].hunks[0].lines.reserve_exact(64);
        diff.files[0].hunks[0].lines[0]
            .text
            .reserve_exact(64 * 1024);
        diff.files[0].metadata.reserve_exact(8);
        diff.files[0].metadata.push(Vec::with_capacity(128 * 1024));
        let after = super::diff(&diff).unwrap();
        assert!(after >= before + 192 * 1024);
        assert_eq!(diff.files[0].hunks[0].lines[0].text, b"old");
        assert!(diff.files[0].metadata[0].is_empty());
        diff.files[0].metadata.clear();
        assert_eq!(
            format!("{diff:?}"),
            text_before,
            "Only actual spare capacities changed"
        );
        assert!(super::diff(&diff).unwrap() >= before + 64 * 1024);
        assert_eq!(
            sum([usize::MAX, 1]),
            None,
            "Overflow is refusal, never a free graph"
        );
    }
}
