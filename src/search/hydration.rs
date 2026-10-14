//! Worker-only projection of bounded, verified gh snapshots into canonical discussion sources.
//! This module never performs authentication or transport and never broadens target selection.
use super::{
    collaboration::{
        Access, DiscussionEntry, DiscussionSnapshot, DiscussionTarget, ParentKind, ThreadInput,
    },
    model::{DiscussionContext, DiscussionKind},
    QueryError,
};
use crate::{
    providers::gh::{
        DiscussionParent, DiscussionParentKind, DiscussionRecordKind,
        DiscussionSnapshot as ProviderSnapshot, DiscussionThreadsSnapshot, PrSummary, Repository,
    },
    storage::Scope,
};
use std::collections::{BTreeMap, BTreeSet};

/// Targets can be reserved from an already-visible exact PR before slow category hydration.
pub fn pr_targets(
    scope: Scope,
    repository: &Repository,
    pr: &PrSummary,
) -> Result<Vec<DiscussionTarget>, QueryError> {
    if pr.pr.host != scope.host || !pr.pr.repository.eq_ignore_ascii_case(&repository.full_name) {
        return Err(error(
            "PR source belongs to another selected host/repository",
        ));
    }
    targets(
        scope,
        repository,
        ParentKind::PullRequest,
        &pr.pr.id,
        pr.pr.number,
    )
}

/// Issue targets come from the bounded canonical issue_parent preflight, not after body fetching.
pub fn parent_targets(
    scope: Scope,
    repository: &Repository,
    parent: &DiscussionParent,
) -> Result<Vec<DiscussionTarget>, QueryError> {
    validate_parent(&scope, repository, parent)?;
    targets(
        scope,
        repository,
        parent_kind(parent.kind),
        &parent.node_id,
        parent.number,
    )
}

pub fn from_records(
    scope: Scope,
    repository: &Repository,
    provider: &ProviderSnapshot,
    category: DiscussionKind,
    fetched_at: u64,
) -> Result<Vec<DiscussionSnapshot>, QueryError> {
    super::remote::verify_scope(&scope, &provider.identity)?;
    validate_parent(&scope, repository, &provider.parent)?;
    let expected = match category {
        DiscussionKind::ConversationComment => DiscussionRecordKind::ConversationComment,
        DiscussionKind::Review if provider.parent.kind == DiscussionParentKind::PullRequest => {
            DiscussionRecordKind::Review
        }
        _ => {
            return Err(error(
                "Record hydration needs its explicit conversation/review category",
            ))
        }
    };
    if provider.coverage.loaded != provider.records.len() {
        return Err(error(
            "Provider category coverage differs from its admitted canonical records",
        ));
    }
    let body = body_snapshot(scope.clone(), repository, &provider.parent, fetched_at)?;
    let mut source = empty_snapshot(scope, repository, &provider.parent, category, fetched_at)?;
    source.enumeration_complete = provider.coverage.enumeration_complete;
    source.complete = provider.coverage.complete;
    source.reasons = provider.coverage.reasons.clone();
    mark_unknown_labels(&mut source);
    source.excluded_nodes = provider
        .coverage
        .excluded_nodes
        .iter()
        .filter(|node| *node != &provider.parent.node_id)
        .cloned()
        .collect();
    for record in &provider.records {
        if record.kind != expected {
            return Err(error("Provider returned a different discussion category"));
        }
        let title = match record.kind {
            DiscussionRecordKind::ConversationComment => format!(
                "{} #{} conversation comment",
                parent_label(provider.parent.kind),
                provider.parent.number
            ),
            DiscussionRecordKind::Review => format!(
                "PR #{} review {}",
                provider.parent.number,
                record.state.as_deref().unwrap_or("unknown decision")
            ),
        };
        let mut context = context(
            &source.target,
            &record.node_id,
            record.numeric_id,
            Some(record.url.clone()),
        );
        context.commit_sha = record.commit_sha.clone();
        source.entries.push(DiscussionEntry {
            node_id: record.node_id.clone(),
            numeric_id: record.numeric_id,
            title,
            body: record.body.clone(),
            author: record.author.clone(),
            created_at: match record.kind {
                DiscussionRecordKind::ConversationComment => record.created_at.clone(),
                DiscussionRecordKind::Review => record
                    .created_at
                    .clone()
                    .or_else(|| record.submitted_at.clone()),
            },
            updated_at: record.updated_at.clone(),
            context,
        });
    }
    source.validate()?; // Reject mixed context without duplicating bounded source bodies.
    Ok(vec![body, source])
}

pub fn from_threads(
    scope: Scope,
    repository: &Repository,
    provider: &DiscussionThreadsSnapshot,
    fetched_at: u64,
) -> Result<Vec<DiscussionSnapshot>, QueryError> {
    super::remote::verify_scope(&scope, &provider.identity)?;
    validate_parent(&scope, repository, &provider.parent)?;
    if provider.parent.kind != DiscussionParentKind::PullRequest
        || provider.coverage.loaded != provider.threads.len()
        || provider.repository.id != repository.id
        || provider.repository.host != repository.host
        || !provider
            .repository
            .full_name
            .eq_ignore_ascii_case(&repository.full_name)
        || provider.target.pr.id != provider.parent.node_id
        || provider.target.pr.number != provider.parent.number
        || provider.parent.head_sha.as_ref() != Some(&provider.target.head_sha)
    {
        return Err(error(
            "Inline source belongs to another canonical PR/repository/head",
        ));
    }
    let body = body_snapshot(scope.clone(), repository, &provider.parent, fetched_at)?;
    let mut inner = BTreeMap::new();
    let mut reasons = provider.coverage.reasons.clone();
    for (node, coverage) in &provider.comment_coverage {
        let Some(thread) = provider.threads.iter().find(|thread| &thread.id == node) else {
            return Err(error("Inner enumeration belongs to an unobserved thread"));
        };
        if coverage.loaded != thread.comments.len() {
            return Err(error(
                "Inner coverage differs from its admitted canonical comments",
            ));
        }
        inner.insert(node.clone(), coverage.enumeration_complete);
        reasons.extend(
            coverage
                .reasons
                .iter()
                .map(|reason| format!("Thread {node}: {reason}")),
        );
    }
    let mut source = empty_snapshot(
        scope,
        repository,
        &provider.parent,
        DiscussionKind::InlineComment,
        fetched_at,
    )?
    .with_threads(ThreadInput {
        threads: &provider.threads,
        outer_complete: provider.coverage.enumeration_complete,
        complete: provider.coverage.complete,
        comment_enumeration: &inner,
        reasons: &reasons,
    })?;
    source.excluded_threads = provider
        .coverage
        .excluded_nodes
        .iter()
        .filter(|node| *node != &provider.parent.node_id)
        .cloned()
        .collect();
    for coverage in provider.comment_coverage.values() {
        source
            .excluded_nodes
            .extend(coverage.excluded_nodes.iter().cloned());
    }
    mark_unknown_labels(&mut source);
    source.validate()?;
    Ok(vec![body, source])
}

/// Category fetches may repeat the same parent body, but cannot silently combine changed parent
/// content or publication targets. A worker passes the same fetched_at for its explicit batch.
pub fn coalesce(snapshots: Vec<DiscussionSnapshot>) -> Result<Vec<DiscussionSnapshot>, QueryError> {
    let mut sources = BTreeMap::<String, DiscussionSnapshot>::new();
    for snapshot in snapshots {
        snapshot.target.validate()?;
        let key = snapshot.target.source();
        if let Some(previous) = sources.get_mut(&key) {
            if !matches!(
                snapshot.target.category,
                DiscussionKind::PullRequestBody | DiscussionKind::IssueBody
            ) || previous.target != snapshot.target
                || previous.head_sha != snapshot.head_sha
                || previous.parent_state != snapshot.parent_state
                || previous.access != snapshot.access
                || previous.parent_labels != snapshot.parent_labels
                || previous.excluded_nodes != snapshot.excluded_nodes
                || previous.fetched_at != snapshot.fetched_at
                || serde_json::to_value(&previous.entries).map_err(error)?
                    != serde_json::to_value(&snapshot.entries).map_err(error)?
            {
                return Err(error(
                    "Hydration batch contains duplicate or changed canonical source bodies",
                ));
            }
            previous.complete &= snapshot.complete;
            previous.reasons.extend(snapshot.reasons);
            previous.reasons.sort();
            previous.reasons.dedup();
        } else {
            sources.insert(key, snapshot);
        }
    }
    // Independent categories must also identify the same observed parent version for each node.
    let mut parents = BTreeMap::new();
    for snapshot in sources.values() {
        let parent = (
            &snapshot.target.scope.host,
            &snapshot.target.scope.login,
            snapshot.target.repository_id,
            &snapshot.target.parent_node,
        );
        let version = (
            &snapshot.head_sha,
            &snapshot.parent_state,
            &snapshot.parent_labels,
        );
        if parents
            .insert(parent, version)
            .is_some_and(|old| old != version)
        {
            return Err(error(
                "Hydration categories observed different canonical parent context",
            ));
        }
    }
    drop(parents);
    Ok(sources.into_values().collect())
}

fn body_snapshot(
    scope: Scope,
    repository: &Repository,
    parent: &DiscussionParent,
    fetched_at: u64,
) -> Result<DiscussionSnapshot, QueryError> {
    let category = match parent.kind {
        DiscussionParentKind::PullRequest => DiscussionKind::PullRequestBody,
        DiscussionParentKind::Issue => DiscussionKind::IssueBody,
    };
    let mut source = empty_snapshot(scope, repository, parent, category, fetched_at)?;
    source.enumeration_complete = true; // Exactly one canonical parent was observed.
    source.complete = parent.body_complete;
    if parent.body_complete {
        source.entries.push(DiscussionEntry {
            node_id: parent.node_id.clone(),
            numeric_id: parent.numeric_id,
            title: parent.title.clone(),
            body: parent.body.clone(),
            author: parent.author.clone(),
            created_at: parent.created_at.clone(),
            updated_at: parent.updated_at.clone(),
            context: context(
                &source.target,
                &parent.node_id,
                parent.numeric_id,
                Some(parent.url.clone()),
            ),
        });
    } else {
        source.excluded_nodes.insert(parent.node_id.clone());
        source.reasons.push("Observed parent body exceeds hydration budget; neither cleared placeholder nor previous body is searchable".into());
    }
    mark_unknown_labels(&mut source);
    source.validate()?;
    Ok(source)
}

fn empty_snapshot(
    scope: Scope,
    repository: &Repository,
    parent: &DiscussionParent,
    category: DiscussionKind,
    fetched_at: u64,
) -> Result<DiscussionSnapshot, QueryError> {
    validate_parent(&scope, repository, parent)?;
    let target = DiscussionTarget {
        scope,
        repository_id: repository.id,
        repository: repository.full_name.clone(),
        parent_kind: parent_kind(parent.kind),
        parent_node: parent.node_id.clone(),
        number: parent.number,
        category,
    };
    target.validate()?;
    Ok(DiscussionSnapshot {
        target,
        entries: vec![],
        access: Access::Allowed,
        enumeration_complete: false,
        complete: false,
        reasons: vec![],
        fetched_at,
        head_sha: parent.head_sha.clone(),
        parent_state: Some(parent.state.clone()),
        parent_labels: parent.labels.clone(),
        excluded_nodes: BTreeSet::new(),
        excluded_threads: BTreeSet::new(),
        thread_enumeration: None,
    })
}
fn targets(
    scope: Scope,
    repository: &Repository,
    kind: ParentKind,
    node: &str,
    number: u64,
) -> Result<Vec<DiscussionTarget>, QueryError> {
    let categories = if kind == ParentKind::PullRequest {
        vec![
            DiscussionKind::PullRequestBody,
            DiscussionKind::ConversationComment,
            DiscussionKind::Review,
            DiscussionKind::InlineComment,
        ]
    } else {
        vec![
            DiscussionKind::IssueBody,
            DiscussionKind::ConversationComment,
        ]
    };
    categories
        .into_iter()
        .map(|category| {
            let target = DiscussionTarget {
                scope: scope.clone(),
                repository_id: repository.id,
                repository: repository.full_name.clone(),
                parent_kind: kind,
                parent_node: node.into(),
                number,
                category,
            };
            target.validate()?;
            if repository.host != scope.host {
                return Err(error(
                    "Selected repository belongs to another verified host",
                ));
            }
            Ok(target)
        })
        .collect()
}
fn validate_parent(
    scope: &Scope,
    repository: &Repository,
    parent: &DiscussionParent,
) -> Result<(), QueryError> {
    if repository.id == 0
        || parent.repository_id != repository.id
        || parent.host != scope.host
        || repository.host != scope.host
        || !parent
            .repository
            .eq_ignore_ascii_case(&repository.full_name)
        || parent.numeric_id == Some(0)
        || parent
            .head_sha
            .as_ref()
            .is_some_and(|id| !super::model::valid_sha(id))
    {
        return Err(error(
            "Discussion parent differs from the positive selected repository/account context",
        ));
    }
    Ok(())
}
fn parent_kind(kind: DiscussionParentKind) -> ParentKind {
    match kind {
        DiscussionParentKind::PullRequest => ParentKind::PullRequest,
        DiscussionParentKind::Issue => ParentKind::Issue,
    }
}
fn parent_label(kind: DiscussionParentKind) -> &'static str {
    match kind {
        DiscussionParentKind::PullRequest => "PR",
        DiscussionParentKind::Issue => "Issue",
    }
}
fn context(
    target: &DiscussionTarget,
    node: &str,
    numeric_id: Option<u64>,
    url: Option<String>,
) -> DiscussionContext {
    DiscussionContext {
        kind: target.category,
        repository_id: target.repository_id,
        parent_node: target.parent_node.clone(),
        parent_number: target.number,
        resource_node: node.into(),
        resource_numeric_id: numeric_id,
        thread_id: None,
        path: None,
        commit_sha: None,
        side: None,
        line: None,
        start_line: None,
        outdated: false,
        resolved: None,
        url,
    }
}
fn error(value: impl std::fmt::Display) -> QueryError {
    QueryError(value.to_string())
}
fn mark_unknown_labels(snapshot: &mut DiscussionSnapshot) {
    if snapshot.parent_labels.is_none() {
        snapshot.complete = false;
        snapshot
            .reasons
            .push("Parent labels unavailable; no empty-label state inferred".into());
    }
}
