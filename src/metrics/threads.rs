//! Staged separately: no legacy ReviewThread adapter can manufacture original anchors.
//! Root must freshly authorize the canonical parent before publishing counts or opening rows.
use super::{
    CoverageState, EvidenceCoverage, MetricActor, MetricValue, MetricsScope, PrCohortKind, PrTarget,
};
use crate::{search::model::valid_sha, storage::Scope};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const MAX_THREADS: usize = 10_000;
const MAX_PARTICIPANTS: usize = 50_000;
const MAX_METADATA_BYTES: usize = 32 * 1024 * 1024;

fn anchor_bytes(anchor: Option<&ThreadAnchor>) -> usize {
    anchor.map_or(0, |anchor| {
        anchor.path.len() + anchor.commit_sha.as_ref().map_or(0, String::len)
    })
}
fn retained_metadata_bytes(thread: &ThreadObservation) -> usize {
    let mut bytes = thread.target.thread_node_id.len()
        + thread.target.current_head_sha.len()
        + thread.target.pr.repository.len()
        + thread.target.pr.node_id.as_ref().map_or(0, String::len)
        + thread
            .target
            .pr
            .source_record_id
            .as_ref()
            .map_or(0, String::len)
        + anchor_bytes(thread.target.current_anchor.as_ref());
    if let Some(original) = &thread.target.original_anchor {
        bytes += original.comment_node_id.len()
            + original.observed_path.as_ref().map_or(0, String::len)
            + original.commit_sha.as_ref().map_or(0, String::len)
            + anchor_bytes(original.verified_anchor.as_ref());
    }
    for participant in &thread.participants {
        bytes = bytes.saturating_add(
            participant.comment_node_id.as_ref().map_or(0, String::len)
                + participant.actor.actor.login.len()
                + participant.actor.actor.actor_type.len()
                + participant
                    .actor
                    .actor
                    .display_name
                    .as_ref()
                    .map_or(0, String::len)
                + participant
                    .actor
                    .actor
                    .avatar_url
                    .as_ref()
                    .map_or(0, String::len),
        );
    }
    bytes
}

fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 4096
        && !path.starts_with('/')
        && !path.contains('\0')
        && !path.split('/').any(|part| matches!(part, ".." | "." | ""))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ThreadSide {
    Left,
    Right,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadAnchor {
    pub path: String,
    pub side: ThreadSide,
    /// Actual originalLine/current line only; null is not inferred from the other anchor.
    pub line: Option<u64>,
    pub start_line: Option<u64>,
    /// Actual original comment commit, never filled with the current PR head.
    pub commit_sha: Option<String>,
}
impl ThreadAnchor {
    fn validate(&self) -> Result<(), String> {
        if !valid_path(&self.path)
            || self.line == Some(0)
            || self.start_line == Some(0)
            || self
                .start_line
                .is_some_and(|start| self.line.is_none_or(|end| start > end))
            || self.commit_sha.as_ref().is_some_and(|sha| !valid_sha(sha))
        {
            return Err("Thread anchor requires its safe raw path, valid nullable range and actual full commit".into());
        }
        Ok(())
    }
    pub fn complete(&self) -> bool {
        self.line.is_some_and(|line| line > 0)
            && self.commit_sha.as_ref().is_some_and(|sha| valid_sha(sha))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadOriginalAnchor {
    pub comment_node_id: String,
    pub line: Option<u64>,
    pub start_line: Option<u64>,
    pub commit_sha: Option<String>,
    /// GitHub has no originalPath field. Observed comment placement is only a candidate.
    pub observed_path: Option<String>,
    pub observed_side: Option<ThreadSide>,
    /// Supplied only by an independent exact original-commit/tree/path verifier.
    /// These public pure projections never themselves grant private read authority.
    pub verified_anchor: Option<ThreadAnchor>,
}
impl ThreadOriginalAnchor {
    fn validate(&self) -> Result<(), String> {
        if !valid_node(&self.comment_node_id)
            || self.line == Some(0)
            || self.start_line == Some(0)
            || self
                .start_line
                .is_some_and(|start| self.line.is_none_or(|end| start > end))
            || self.commit_sha.as_ref().is_some_and(|sha| !valid_sha(sha))
        {
            return Err("Original thread metadata requires its genuine comment node and nullable actual range/commit".into());
        }
        if self
            .observed_path
            .as_ref()
            .is_some_and(|path| !valid_path(path))
        {
            return Err("Observed thread path candidate is not a safe bounded raw path".into());
        }
        if let Some(anchor) = &self.verified_anchor {
            anchor.validate()?;
            if !anchor.complete()
                || anchor.commit_sha != self.commit_sha
                || anchor.line != self.line
                || anchor.start_line != self.start_line
            {
                return Err(
                    "Verified old-tree anchor must retain exact original comment commit/range"
                        .into(),
                );
            }
        }
        Ok(())
    }
    pub fn complete(&self) -> bool {
        self.verified_anchor
            .as_ref()
            .is_some_and(ThreadAnchor::complete)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadTarget {
    pub pr: PrTarget,
    pub thread_node_id: String,
    pub current_head_sha: String,
    pub original_anchor: Option<ThreadOriginalAnchor>,
    pub current_anchor: Option<ThreadAnchor>,
}
fn valid_node(node: &str) -> bool {
    !node.is_empty() && node.len() <= 512 && !node.chars().any(char::is_control)
}
impl ThreadTarget {
    pub fn validate(&self, scope: &MetricsScope) -> Result<(), String> {
        self.pr.validate(scope)?;
        if !self.pr.has_canonical_identity()
            || self
                .pr
                .node_id
                .as_ref()
                .is_none_or(|node| !valid_node(node))
            || self
                .pr
                .source_record_id
                .as_ref()
                .is_some_and(|id| id.len() > 512)
            || !valid_node(&self.thread_node_id)
            || !valid_sha(&self.current_head_sha)
        {
            return Err(
                "Thread metric requires genuine canonical parent/node and full current head".into(),
            );
        }
        if let Some(anchor) = &self.original_anchor {
            anchor.validate()?;
        }
        if let Some(anchor) = &self.current_anchor {
            anchor.validate()?;
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThreadActorRole {
    OriginalCommentAuthor,
    ReplyAuthor,
    Resolver,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadParticipant {
    pub role: ThreadActorRole,
    /// Actual source comment node for comment authors, not position in an array.
    pub comment_node_id: Option<String>,
    pub actor: MetricActor,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadObservation {
    pub target: ThreadTarget,
    /// Nullable observations prevent missing GraphQL fields becoming false.
    pub resolved: Option<bool>,
    pub outdated: Option<bool>,
    pub participants: Vec<ThreadParticipant>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadParent {
    pub target: PrTarget,
    pub head_sha: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThreadCollection {
    pub scope: Scope,
    pub parent: ThreadParent,
    pub threads: Vec<ThreadObservation>,
    /// Coverage of exact outer thread STATE nodes, not of nested comments or PR files.
    pub state_coverage: EvidenceCoverage,
    pub provenance_coverage: EvidenceCoverage,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThreadClass {
    UnresolvedOutdated,
    UnresolvedCurrent,
    ResolvedOutdated,
    ResolvedCurrent,
    Unknown,
}
fn classify(thread: &ThreadObservation) -> ThreadClass {
    match (thread.resolved, thread.outdated) {
        (Some(false), Some(true)) => ThreadClass::UnresolvedOutdated,
        (Some(false), Some(false)) => ThreadClass::UnresolvedCurrent,
        (Some(true), Some(true)) => ThreadClass::ResolvedOutdated,
        (Some(true), Some(false)) => ThreadClass::ResolvedCurrent,
        _ => ThreadClass::Unknown,
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThreadContributor {
    pub target: ThreadTarget,
    pub class: ThreadClass,
    pub participants: Vec<ThreadParticipant>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThreadMetricCard {
    pub label: String,
    pub formula: String,
    pub cohort: String,
    pub value: MetricValue,
    pub coverage: EvidenceCoverage,
    pub provenance_coverage: EvidenceCoverage,
    pub unresolved_known: u64,
    pub outdated_unresolved_known: u64,
    pub unknown_state: u64,
    pub contributors: Vec<ThreadContributor>,
    pub missing_parents: Vec<PrTarget>,
}
fn merge_coverage(into: &mut EvidenceCoverage, source: &EvidenceCoverage, family: &str) {
    match &source.state {
        CoverageState::Complete => {}
        CoverageState::Partial(reasons) => {
            into.partial(format!("{family}: {}", reasons.join("; ")));
        }
        CoverageState::Unavailable(reason) => into.partial(format!("{family}: {reason}")),
    }
}
/// Pure current-snapshot metric. No age threshold, retained trend or absence from a cap.
pub fn unresolved_outdated_threads(
    scope: &MetricsScope,
    parents: &[ThreadParent],
    cohort: PrCohortKind,
    cohort_coverage: &EvidenceCoverage,
    collections: &[ThreadCollection],
    as_of: u64,
) -> Result<ThreadMetricCard, String> {
    scope.validate()?;
    if parents.len() > MAX_THREADS || collections.len() > MAX_THREADS {
        return Err("Thread metric parent budget exceeded".into());
    }
    if !parents.is_empty()
        && (cohort_coverage.complete_empty
            || matches!(cohort_coverage.state, CoverageState::Unavailable(_)))
    {
        return Err("Unavailable/complete-empty cohort cannot admit thread parents".into());
    }
    if cohort_coverage.fetched_at.is_some_and(|at| at > as_of) {
        return Err("PR cohort was observed after thread metric as-of".into());
    }
    let stable_scope =
        Scope::github(scope.host(), Some(scope.account_id())).map_err(|error| error.to_string())?;
    let mut expected = BTreeMap::new();
    for parent in parents {
        parent.target.validate(scope)?;
        if parent.head_sha.as_ref().is_some_and(|sha| !valid_sha(sha)) {
            return Err("Thread parent head must be a full source commit".into());
        }
        let key = (
            parent.target.repository.to_ascii_lowercase(),
            parent.target.number,
        );
        if expected
            .insert(key, parent)
            .is_some_and(|old| old != parent)
        {
            return Err("Conflicting canonical parents in thread cohort".into());
        }
    }
    let mut coverage = cohort_coverage.clone();
    let mut provenance_coverage = cohort_coverage.clone();
    if parents.is_empty() && !cohort_coverage.complete_empty {
        coverage = EvidenceCoverage::unavailable("No complete-empty PR cohort proof");
        provenance_coverage = coverage.clone();
    }
    let mut seen_parents = BTreeSet::new();
    let mut nodes = BTreeMap::<String, &ThreadObservation>::new();
    let mut comment_threads = BTreeMap::<String, String>::new();
    let mut participants = 0usize;
    let mut thread_rows = 0usize;
    let mut metadata_bytes = 0usize;
    for collection in collections {
        let key = (
            collection.parent.target.repository.to_ascii_lowercase(),
            collection.parent.target.number,
        );
        if collection.scope != stable_scope
            || expected.get(&key).copied() != Some(&collection.parent)
            || !collection.parent.target.has_canonical_identity()
            || collection.parent.head_sha.is_none()
            || !seen_parents.insert(key)
        {
            return Err("Thread collection has a foreign/replaced principal, parent, head or repeated source".into());
        }
        thread_rows = thread_rows.saturating_add(collection.threads.len());
        if thread_rows > MAX_THREADS {
            return Err("Thread metric row budget exceeded".into());
        }
        if !collection.threads.is_empty()
            && (collection.state_coverage.complete_empty
                || matches!(
                    collection.state_coverage.state,
                    CoverageState::Unavailable(_)
                ))
        {
            return Err(
                "Unavailable/complete-empty outer source cannot admit thread states".into(),
            );
        }
        if collection
            .state_coverage
            .fetched_at
            .is_some_and(|at| at > as_of)
        {
            return Err("Thread snapshot was observed after metric as-of".into());
        }
        merge_coverage(
            &mut coverage,
            &collection.state_coverage,
            "Outer thread states",
        );
        merge_coverage(
            &mut provenance_coverage,
            &collection.state_coverage,
            "Outer thread states",
        );
        merge_coverage(
            &mut provenance_coverage,
            &collection.provenance_coverage,
            "Thread actors/content",
        );
        if collection.state_coverage.fetched_at.is_none() {
            coverage.partial("Thread observation time unavailable");
            provenance_coverage.partial("Thread observation time unavailable");
        }
        if collection.threads.is_empty() && !collection.state_coverage.complete_empty {
            coverage.partial("Empty thread list lacks explicit complete-empty outer enumeration");
        }
        for thread in &collection.threads {
            participants = participants.saturating_add(thread.participants.len());
            if participants > MAX_PARTICIPANTS {
                return Err("Thread actor budget exceeded".into());
            }
            thread.target.validate(scope)?;
            metadata_bytes = metadata_bytes.saturating_add(retained_metadata_bytes(thread));
            if metadata_bytes > MAX_METADATA_BYTES {
                return Err("Thread metric retained metadata byte budget exceeded".into());
            }
            if thread.target.pr != collection.parent.target
                || collection.parent.head_sha.as_deref()
                    != Some(thread.target.current_head_sha.as_str())
            {
                return Err(
                    "Thread node belongs to a different canonical parent/current head".into(),
                );
            }
            if let Some(old) = nodes.insert(thread.target.thread_node_id.clone(), thread) {
                if old != thread {
                    return Err(
                        "Conflicting state or provenance for one genuine thread node".into(),
                    );
                }
            }
            let mut comment_nodes = BTreeSet::new();
            for participant in &thread.participants {
                if participant.actor.account_id == Some(0)
                    || participant.actor.actor.login.len() > 512
                    || participant
                        .actor
                        .actor
                        .display_name
                        .as_ref()
                        .is_some_and(|name| name.len() > 4096)
                    || participant
                        .actor
                        .actor
                        .avatar_url
                        .as_ref()
                        .is_some_and(|url| url.len() > 4096)
                    || participant.actor.actor.actor_type.len() > 128
                    || participant
                        .comment_node_id
                        .as_ref()
                        .is_some_and(|node| !valid_node(node) || !comment_nodes.insert(node))
                    || (participant.role != ThreadActorRole::Resolver
                        && participant.comment_node_id.is_none())
                {
                    return Err(
                        "Thread participant lacks bounded genuine comment/actor provenance".into(),
                    );
                }
                if let Some(comment) = &participant.comment_node_id {
                    if comment_threads
                        .insert(comment.clone(), thread.target.thread_node_id.clone())
                        .is_some_and(|owner| owner != thread.target.thread_node_id)
                    {
                        return Err(
                            "One genuine comment node cannot belong to two thread contributors"
                                .into(),
                        );
                    }
                }
            }
            if let Some(original) = &thread.target.original_anchor {
                if comment_threads
                    .insert(
                        original.comment_node_id.clone(),
                        thread.target.thread_node_id.clone(),
                    )
                    .is_some_and(|owner| owner != thread.target.thread_node_id)
                {
                    return Err(
                        "Original comment node belongs to a different genuine thread".into(),
                    );
                }
            }
            if thread
                .target
                .original_anchor
                .as_ref()
                .is_none_or(|anchor| !anchor.complete())
            {
                provenance_coverage.partial("Original thread anchor or original commit is unavailable; no current-anchor substitution");
            }
            if thread.participants.is_empty() {
                provenance_coverage.partial("Thread actor records unavailable");
            }
        }
    }
    let missing_parents = expected
        .into_iter()
        .filter_map(|(key, parent)| (!seen_parents.contains(&key)).then(|| parent.target.clone()))
        .collect::<Vec<_>>();
    if !missing_parents.is_empty() {
        coverage.partial("One or more loaded PRs lack a current exact thread-state inventory");
        provenance_coverage.partial("One or more loaded PRs lack thread provenance");
    }
    let contributors = nodes
        .values()
        .map(|thread| ThreadContributor {
            target: thread.target.clone(),
            class: classify(thread),
            participants: thread.participants.clone(),
        })
        .collect::<Vec<_>>();
    let outdated_unresolved_known = contributors
        .iter()
        .filter(|row| row.class == ThreadClass::UnresolvedOutdated)
        .count() as u64;
    let unresolved_known = nodes
        .values()
        .filter(|thread| thread.resolved == Some(false))
        .count() as u64;
    let unknown_state = contributors
        .iter()
        .filter(|row| row.class == ThreadClass::Unknown)
        .count() as u64;
    if unknown_state > 0 {
        coverage.partial("Resolution/outdated state is missing for observed thread nodes");
    }
    let value =
        if outdated_unresolved_known > 0 || matches!(coverage.state, CoverageState::Complete) {
            MetricValue::Count(outdated_unresolved_known)
        } else {
            MetricValue::Unavailable
        };
    Ok(ThreadMetricCard {
        label: "Unresolved outdated threads".into(),
        formula: "Count distinct canonical thread nodes with observed isResolved=false AND isOutdated=true. Current loaded PR snapshot; not time-based staleness or a UTC-window historical trend.".into(),
        cohort: match cohort {
            PrCohortKind::PersonalQueueUnion => "Loaded personal authored/assigned/review-requested PR union; not repository-wide",
            PrCohortKind::SelectedRepositoryInventory => "Loaded explicit selected-repository PR inventory",
        }.into(),
        value,
        coverage,
        provenance_coverage,
        unresolved_known,
        outdated_unresolved_known,
        unknown_state,
        contributors,
        missing_parents,
    })
}

#[cfg(test)]
#[path = "threads_tests.rs"]
mod tests;
