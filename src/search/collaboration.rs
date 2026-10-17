//! Canonical discussion generations. Worker-only; no API/authentication implementation here.
mod binding_visibility;
pub use binding_visibility::{binding_metadata_visibility, BindingMetadataVisibility};

use super::{
    corpus::Corpus,
    model::{
        Coverage, Destination, DiscussionContext, DiscussionKind, Document, DocumentKey, Metadata,
        ResultKind, SourceKind,
    },
    QueryError,
};
use crate::{
    providers::{
        gh::{Actor, PrDetails, Repository, ReviewThread},
        runner::CancellationToken,
    },
    storage::{
        Cache, Record, ReservedSyncBatch, Scope, SyncBinding, SyncBindingKind, SyncFence,
        SyncFenceResult, SyncNamespace,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

const DOCUMENT: &str = "collaboration-document-v1";
const STATE: &str = "collaboration-state-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParentKind {
    PullRequest,
    Issue,
}

/// A category is a separate source: truncated inline paging cannot retain a deleted general comment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscussionTarget {
    pub scope: Scope,
    pub repository_id: u64,
    pub repository: String,
    pub parent_kind: ParentKind,
    pub parent_node: String,
    pub number: u64,
    pub category: DiscussionKind,
}
impl DiscussionTarget {
    pub fn validate(&self) -> Result<(), QueryError> {
        positive_scope(&self.scope)?;
        crate::config::validate_repository(&self.repository).map_err(error)?;
        opaque(&self.parent_node)?;
        if self.repository_id == 0 || self.number == 0 {
            return Err(error(
                "Discussion source needs positive canonical repository and parent identities",
            ));
        }
        if self.parent_kind == ParentKind::Issue
            && matches!(
                self.category,
                DiscussionKind::PullRequestBody
                    | DiscussionKind::Review
                    | DiscussionKind::InlineComment
            )
            || self.parent_kind == ParentKind::PullRequest
                && self.category == DiscussionKind::IssueBody
        {
            return Err(error(
                "Discussion category belongs to a different parent kind",
            ));
        }
        Ok(())
    }
    pub fn source(&self) -> String {
        let identity = (
            &self.scope,
            self.repository_id,
            self.repository.to_ascii_lowercase(),
            self.parent_kind,
            &self.parent_node,
            self.category,
        );
        format!(
            "collaboration-v1:{:x}",
            Sha256::digest(serde_json::to_vec(&identity).expect("Typed identities serialize"))
        )
    }
    pub fn binding(&self) -> Result<SyncBinding, QueryError> {
        self.validate()?;
        Ok(SyncBinding {
            namespace: SyncNamespace::Collaboration,
            repository_id: self.repository_id,
            repository: self.repository.to_ascii_lowercase(),
            resource: self.parent_node.clone(),
            kind: match self.category {
                DiscussionKind::PullRequestBody => SyncBindingKind::PullRequestBody,
                DiscussionKind::IssueBody => SyncBindingKind::IssueBody,
                DiscussionKind::ConversationComment => SyncBindingKind::ConversationComment,
                DiscussionKind::Review => SyncBindingKind::Review,
                DiscussionKind::InlineComment => SyncBindingKind::InlineComment,
            },
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Access {
    Allowed,
    Revoked(String),
    Unavailable(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscussionEntry {
    pub node_id: String,
    pub numeric_id: Option<u64>,
    pub title: String,
    pub body: String,
    pub author: Option<Actor>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub context: DiscussionContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscussionSnapshot {
    pub target: DiscussionTarget,
    pub entries: Vec<DiscussionEntry>,
    pub access: Access,
    pub enumeration_complete: bool,
    pub complete: bool,
    pub reasons: Vec<String>,
    pub fetched_at: u64,
    pub head_sha: Option<String>,
    pub parent_state: Option<String>,
    #[serde(default)]
    pub parent_labels: Option<Vec<String>>,
    /// Canonical nodes seen by the provider but deliberately not admitted by a content budget.
    /// They must not fall back to an earlier cached body under partial enumeration.
    #[serde(default)]
    pub excluded_nodes: BTreeSet<String>,
    /// Observed outer thread groups excluded before their current content could be admitted.
    #[serde(default)]
    pub excluded_threads: BTreeSet<String>,
    /// Category-wide completeness is not enough to describe paged inline discussions.
    /// These identities authorize pruning only their explicitly enumerated thread groups.
    #[serde(default)]
    pub thread_enumeration: Option<ThreadEnumeration>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThreadEnumeration {
    pub observed_threads: BTreeSet<String>,
    pub completed_threads: BTreeSet<String>,
    pub outer_complete: bool,
}
/// The caller must supply outer and per-thread enumeration from the provider's actual pages.
/// A loaded slice alone can never imply complete enumeration of all PR threads.
#[derive(Clone, Copy)]
pub struct ThreadInput<'a> {
    pub threads: &'a [ReviewThread],
    pub outer_complete: bool,
    pub complete: bool,
    pub comment_enumeration: &'a BTreeMap<String, bool>,
    pub reasons: &'a [String],
}
#[derive(Debug, Clone)]
pub struct DiscussionLimits {
    pub max_documents: usize,
    pub max_document_bytes: usize,
    pub max_total_bytes: usize,
}
impl Default for DiscussionLimits {
    fn default() -> Self {
        Self {
            max_documents: 5_000,
            max_document_bytes: 256 * 1024,
            max_total_bytes: 32 * 1024 * 1024,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct State {
    target: DiscussionTarget,
    access: Access,
    enumeration_complete: bool,
    complete: bool,
    reasons: Vec<String>,
    fetched_at: u64,
    head_sha: Option<String>,
    #[serde(default)]
    suppressed_resources: BTreeSet<String>,
    #[serde(default)]
    suppressed_threads: BTreeSet<String>,
}
pub struct CollectedDiscussions {
    pub corpus: Corpus,
    /// Full canonical projections, not IDs alone: edited bodies must fence old lexical matches.
    pub visible_versions: BTreeMap<String, String>,
}

impl DiscussionSnapshot {
    /// The provider REST lists return complete or fail; PrDetails.coverage describes FILE caps,
    /// not discussion coverage. Optional threads have their own honest nested-page completeness.
    pub fn from_pr(
        scope: Scope,
        repository: &Repository,
        details: &PrDetails,
        threads: Option<ThreadInput<'_>>,
        fetched_at: u64,
    ) -> Result<Vec<Self>, QueryError> {
        let pr = &details.summary.pr;
        if repository.id == 0
            || repository.host != scope.host
            || pr.host != scope.host
            || !repository.full_name.eq_ignore_ascii_case(&pr.repository)
        {
            return Err(error(
                "PR discussion snapshot belongs to another canonical host/repository",
            ));
        }
        Self::from_pr_categories_v19(
            scope,
            repository.id,
            &repository.full_name,
            details,
            threads,
            fetched_at,
            &[
                DiscussionKind::PullRequestBody,
                DiscussionKind::ConversationComment,
                DiscussionKind::Review,
                DiscussionKind::InlineComment,
            ],
        )
    }
    /// Actual bounded Conversation refresh admits ONLY its read categories.
    /// It cannot fabricate Review/Inline hydration or empty known labels.
    pub(crate) fn from_conversation_refresh_v19(
        scope: Scope,
        actual: &crate::providers::gh::ConversationDetailSnapshotV19,
        fetched_at: u64,
    ) -> Result<Vec<Self>, QueryError> {
        let parent = actual.parent();
        if parent.summary().pr.host != scope.host
            || actual.details().summary.pr.id != parent.node_id()
        {
            return Err(error(
                "Actual Conversation refresh belongs to another parent/scope",
            ));
        }
        Self::from_pr_categories_v19(
            scope,
            parent.repository_id(),
            parent.repository(),
            actual.details(),
            None,
            fetched_at,
            &[
                DiscussionKind::PullRequestBody,
                DiscussionKind::ConversationComment,
            ],
        )
    }
    fn from_pr_categories_v19(
        scope: Scope,
        repository_id: u64,
        repository: &str,
        details: &PrDetails,
        threads: Option<ThreadInput<'_>>,
        fetched_at: u64,
        categories: &[DiscussionKind],
    ) -> Result<Vec<Self>, QueryError> {
        let pr = &details.summary.pr;
        let target = |category| DiscussionTarget {
            scope: scope.clone(),
            repository_id,
            repository: repository.into(),
            parent_kind: ParentKind::PullRequest,
            parent_node: pr.id.clone(),
            number: pr.number,
            category,
        };
        let mut snapshots = Vec::new();
        for &category in categories {
            let target = target(category);
            target.validate()?;
            let mut snapshot = Self {
                target,
                entries: vec![],
                access: Access::Allowed,
                enumeration_complete: true,
                complete: true,
                reasons: vec![],
                fetched_at,
                head_sha: Some(details.summary.head_sha.clone()),
                parent_state: Some(pr.state.clone()),
                parent_labels: None,
                excluded_nodes: BTreeSet::new(),
                excluded_threads: BTreeSet::new(),
                thread_enumeration: None,
            };
            match category {
                DiscussionKind::PullRequestBody => snapshot.entries.push(entry(
                    &snapshot.target,
                    &pr.id,
                    None,
                    pr.title.clone(),
                    pr.body.clone(),
                    Some(pr.author.clone()),
                    nonempty(&pr.created_at),
                    nonempty(&pr.updated_at),
                    nonempty(&pr.url),
                )),
                DiscussionKind::ConversationComment => {
                    for comment in &details.comments {
                        snapshot.entries.push(entry(
                            &snapshot.target,
                            &comment.node_id,
                            Some(comment.id),
                            format!("PR #{} conversation comment", pr.number),
                            comment.body.clone(),
                            Some(comment.author.clone()),
                            nonempty(&comment.created_at),
                            nonempty(&comment.updated_at),
                            nonempty(&comment.url),
                        ));
                    }
                }
                DiscussionKind::Review => {
                    for review in &details.reviews {
                        let node = review.get("node_id").and_then(Value::as_str).unwrap_or("");
                        let mut found = entry(
                            &snapshot.target,
                            node,
                            review.get("id").and_then(Value::as_u64),
                            format!(
                                "PR #{} review {}",
                                pr.number,
                                review
                                    .get("state")
                                    .and_then(Value::as_str)
                                    .unwrap_or("unknown decision")
                            ),
                            review
                                .get("body")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .into(),
                            actor(&review["user"]),
                            string(review, "submitted_at"),
                            string(review, "updated_at"),
                            string(review, "html_url"),
                        );
                        found.context.commit_sha = string(review, "commit_id");
                        snapshot.entries.push(found);
                    }
                }
                DiscussionKind::InlineComment => match threads.as_ref() {
                    None => {
                        snapshot.enumeration_complete = false;
                        snapshot.complete = false;
                        snapshot
                            .reasons
                            .push("Inline threads not hydrated; no complete-history claim".into());
                    }
                    Some(input) => snapshot = snapshot.with_threads(*input)?,
                },
                DiscussionKind::IssueBody => unreachable!(),
            }
            // PrDetails does not expose canonical labels. Empty is not an observed label list.
            if snapshot.parent_labels.is_none() {
                snapshot.complete = false;
                snapshot.reasons.push("Parent labels unavailable in compatibility PR snapshot; no empty-label state inferred".into());
            }
            snapshots.push(snapshot);
        }
        Ok(snapshots)
    }
    pub fn with_threads(mut self, input: ThreadInput<'_>) -> Result<Self, QueryError> {
        self.target.validate()?;
        if self.target.category != DiscussionKind::InlineComment {
            return Err(error(
                "Thread input requires a canonical inline discussion source",
            ));
        }
        self.entries.clear();
        self.enumeration_complete = input.outer_complete;
        self.complete = input.complete && input.outer_complete;
        self.reasons.extend_from_slice(input.reasons);
        let mut enumeration = ThreadEnumeration {
            outer_complete: input.outer_complete,
            ..Default::default()
        };
        for thread in input.threads {
            opaque(&thread.id)?;
            if !enumeration.observed_threads.insert(thread.id.clone()) {
                return Err(error("Duplicate canonical inline thread identity"));
            }
            let inner_complete = input.comment_enumeration.get(&thread.id) == Some(&true);
            if inner_complete {
                enumeration.completed_threads.insert(thread.id.clone());
            } else {
                self.enumeration_complete = false;
                self.complete = false;
                self.reasons.push(format!(
                    "Thread {} contains only its loaded comment page",
                    thread.id
                ));
            }
            for comment in &thread.comments {
                let node = comment.get("id").and_then(Value::as_str).unwrap_or("");
                let mut found = entry(
                    &self.target,
                    node,
                    numeric_identity(
                        comment
                            .get("fullDatabaseId")
                            .or_else(|| comment.get("databaseId")),
                    ),
                    format!(
                        "PR #{} inline comment • {}",
                        self.target.number, thread.path
                    ),
                    comment
                        .get("body")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .into(),
                    actor(&comment["author"]),
                    string(comment, "createdAt"),
                    string(comment, "updatedAt"),
                    string(comment, "url"),
                );
                found.context.thread_id = Some(thread.id.clone());
                found.context.path =
                    Some(crate::git::RawPath::new(thread.path.as_bytes().to_vec()));
                found.context.commit_sha = comment
                    .get("commit")
                    .and_then(|commit| string(commit, "oid"));
                found.context.side = nonempty(&thread.side);
                found.context.line = thread.line;
                found.context.start_line = thread.start_line;
                found.context.outdated = thread.outdated;
                found.context.resolved = Some(thread.resolved);
                if found.created_at.is_none() || found.updated_at.is_none() {
                    self.complete = false;
                    self.reasons.push("Inline timestamps unavailable; fetch time is not substituted for source time".into());
                }
                self.entries.push(found);
            }
        }
        self.thread_enumeration = Some(enumeration);
        Ok(self)
    }
    /// Validate provenance without copying discussion bodies into temporary Documents.
    pub fn validate(&self) -> Result<(), QueryError> {
        self.target.validate()?;
        if self.entries.len() > 10_000
            || self.excluded_nodes.len() > 10_000
            || self.excluded_threads.len() > 10_000
        {
            return Err(error("Discussion identity budget exceeds 10000 per source"));
        }
        for node in self.excluded_nodes.iter().chain(&self.excluded_threads) {
            opaque(node)?;
        }
        if let Some(enumeration) = &self.thread_enumeration {
            if self.target.category != DiscussionKind::InlineComment
                || !enumeration
                    .completed_threads
                    .is_subset(&enumeration.observed_threads)
            {
                return Err(error(
                    "Thread enumeration belongs to another discussion source",
                ));
            }
            for thread in &enumeration.observed_threads {
                opaque(thread)?;
            }
        }
        if self.access != Access::Allowed {
            return Ok(());
        }
        let mut ids = BTreeSet::new();
        for entry in &self.entries {
            opaque(&entry.node_id)?;
            if entry.numeric_id == Some(0) || !ids.insert(&entry.node_id) {
                return Err(error(
                    "Discussion records need distinct canonical positive identities",
                ));
            }
            let context = &entry.context;
            if context.kind != self.target.category
                || context.repository_id != self.target.repository_id
                || context.parent_node != self.target.parent_node
                || context.parent_number != self.target.number
                || context.resource_node != entry.node_id
                || context.resource_numeric_id != entry.numeric_id
            {
                return Err(error(
                    "Discussion context differs from its canonical source/resource",
                ));
            }
            if context
                .commit_sha
                .as_ref()
                .is_some_and(|id| !super::model::valid_sha(id))
            {
                return Err(error("Discussion anchor commit is not an immutable SHA"));
            }
            if let Some(path) = &context.path {
                super::model::validate_raw_path(path)?;
            }
            if let Some(thread) = &context.thread_id {
                opaque(thread)?;
                if self
                    .thread_enumeration
                    .as_ref()
                    .is_some_and(|enumeration| !enumeration.observed_threads.contains(thread))
                {
                    return Err(error("Inline comment belongs to an unobserved thread"));
                }
            } else if self.target.category == DiscussionKind::InlineComment {
                return Err(error("Inline comment lacks its canonical thread identity"));
            }
        }
        Ok(())
    }
    pub fn documents(&self) -> Result<Vec<Document>, QueryError> {
        self.validate()?;
        if self.access != Access::Allowed {
            return Ok(vec![]);
        }
        self.entries
            .iter()
            .map(|entry| {
                let context = &entry.context;
                let resource = resource(&self.target, &entry.node_id);
                let body_kind = match self.target.category {
                    DiscussionKind::PullRequestBody => ResultKind::PullRequest,
                    DiscussionKind::IssueBody => ResultKind::Issue,
                    _ => ResultKind::Comment,
                };
                let comment_id = if matches!(body_kind, ResultKind::Comment) {
                    Some(entry.node_id.clone())
                } else {
                    None
                };
                let destination = match self.target.parent_kind {
                    ParentKind::PullRequest => Destination::PullRequest {
                        host: self.target.scope.host.clone(),
                        repository: self.target.repository.clone(),
                        number: self.target.number,
                        comment_id,
                        thread_id: context.thread_id.clone(),
                    },
                    ParentKind::Issue => Destination::Issue {
                        host: self.target.scope.host.clone(),
                        repository: self.target.repository.clone(),
                        number: self.target.number,
                        comment_id,
                    },
                };
                let document = Document {
                    key: DocumentKey {
                        scope: self.target.scope.clone(),
                        repository: self.target.repository.clone(),
                        revision: "discussion".into(),
                        kind: body_kind,
                        path: None,
                        resource,
                    },
                    source: SourceKind::Collaboration,
                    title: entry.title.clone(),
                    body: entry.body.clone(),
                    metadata: Metadata {
                        actors: entry.author.iter().cloned().collect(),
                        labels: self.parent_labels.clone().unwrap_or_default(),
                        state: self.parent_state.clone(),
                        created_at: entry.created_at.clone(),
                        updated_at: entry.updated_at.clone(),
                        fetched_at: self.fetched_at,
                        complete: self.complete,
                        discussion: Some(context.clone()),
                        ..Default::default()
                    },
                    destination,
                };
                document.validate(&self.target.scope)?;
                Ok(document)
            })
            .collect()
    }
}

/// Pure bounded preparation for the closed atomic Conversation transaction.
/// Unlike generic partial publication, this profile has complete actual list
/// enumeration, never reads retained old bodies, and refuses rather than
/// publish a guessed subset. `complete=false` still honestly marks unknown
/// parent labels; it does NOT negate actual membership/deletion enumeration.
pub(crate) fn prepare_complete_conversation_records_v19(
    snapshot: &DiscussionSnapshot,
    cancel: &CancellationToken,
) -> Result<Vec<Record>, QueryError> {
    snapshot.validate()?;
    if snapshot.access != Access::Allowed
        || !snapshot.enumeration_complete
        || !matches!(
            snapshot.target.category,
            DiscussionKind::PullRequestBody | DiscussionKind::ConversationComment
        )
        || !snapshot.excluded_nodes.is_empty()
        || !snapshot.excluded_threads.is_empty()
        || snapshot.thread_enumeration.is_some()
    {
        return Err(error(
            "Closed Conversation projection requires its actual complete body/comment enumeration",
        ));
    }
    let mut records = Vec::new();
    let mut total = 0usize;
    for document in snapshot.documents()? {
        if cancel.is_cancelled() {
            return Err(error("Conversation projection preparation cancelled"));
        }
        let payload = serde_json::to_value(&document).map_err(error)?;
        let bytes = serde_json::to_vec(&payload).map_err(error)?.len();
        total = total
            .checked_add(bytes)
            .filter(|n| *n <= 8 * 1024 * 1024)
            .ok_or_else(|| error("Conversation prepared source exceeds8MiB"))?;
        if bytes > 256 * 1024 || records.len() >= 10_000 {
            return Err(error(
                "Conversation prepared document/count bound exceeded; no partial fresh projection",
            ));
        }
        let mut record = Record::new(
            DOCUMENT,
            document.key.stable_id(),
            &snapshot.target.repository,
            payload,
        );
        record.fetched_at = snapshot.fetched_at;
        record.source_updated_at = document.metadata.updated_at;
        record.complete = document.metadata.complete;
        records.push(record);
    }
    let state = State {
        target: snapshot.target.clone(),
        access: snapshot.access.clone(),
        enumeration_complete: true,
        complete: snapshot.complete,
        reasons: snapshot.reasons.clone(),
        fetched_at: snapshot.fetched_at,
        head_sha: snapshot.head_sha.clone(),
        suppressed_resources: BTreeSet::new(),
        suppressed_threads: BTreeSet::new(),
    };
    records.push(state_record(&state)?);
    Ok(records)
}

/// GraphQL BigInt identities may be strings. An out-of-range value keeps its opaque node
/// identity instead of truncating or inventing a smaller REST identity.
fn numeric_identity(value: Option<&Value>) -> Option<u64> {
    value
        .and_then(|value| {
            value.as_u64().or_else(|| {
                value
                    .as_str()
                    .filter(|value| value.bytes().all(|byte| byte.is_ascii_digit()))
                    .and_then(|value| value.parse::<u64>().ok())
            })
        })
        .filter(|value| *value > 0)
}

/// Reserve before slow provider work. Only the current reserved generation may publish.
pub fn reserve(cache: &mut Cache, target: &DiscussionTarget) -> Result<u64, QueryError> {
    cache
        .reserve_resource_sync(&target.scope, &target.source(), &target.binding()?)
        .map_err(error)
}
/// Hide exactly this source category, retaining bodies and user intent. Also fence
/// all in-flight reads under this principal so old derived rank work cannot publish.
pub fn invalidate(
    cache: &mut Cache,
    target: &DiscussionTarget,
) -> Result<SyncFenceResult, QueryError> {
    let binding = target.binding()?;
    cache
        .fence_resource_sync(
            &target.scope,
            &SyncFence {
                namespace: binding.namespace,
                repository_id: Some(binding.repository_id),
                repository: binding.repository,
                resource: binding.resource,
                kinds: vec![binding.kind],
            },
        )
        .map_err(error)
}

/// Metadata-only guard: check the canonical binding and checkpoint before parsing
/// any source body. `active` is a read-reservation flag, not a visibility flag.
fn visible_generation(cache: &Cache, target: &DiscussionTarget) -> Result<Option<u64>, QueryError> {
    let Some(status) = cache
        .binding_status(&target.scope, &target.source())
        .map_err(error)?
    else {
        return Ok(None);
    };
    if status.binding != target.binding()? {
        return Err(error(
            "Cached source binding differs from its exact canonical target",
        ));
    }
    let checkpoint = cache
        .sync_checkpoint(&target.scope, &target.source())
        .map_err(error)?;
    Ok(status
        .visible_generation
        .filter(|generation| checkpoint.is_some_and(|(published, _)| published == *generation)))
}
pub fn publish(
    cache: &mut Cache,
    snapshot: &DiscussionSnapshot,
    generation: u64,
    limits: &DiscussionLimits,
    cancel: &CancellationToken,
) -> Result<bool, QueryError> {
    validate_limits(limits)?;
    snapshot.target.validate()?;
    if cancel.is_cancelled() {
        return Err(error("Discussion hydration cancelled before publication"));
    }
    if cache
        .sync_reservation(&snapshot.target.scope, &snapshot.target.source())
        .map_err(error)?
        != Some(generation)
    {
        return Ok(false);
    }
    let binding = cache
        .binding_status(&snapshot.target.scope, &snapshot.target.source())
        .map_err(error)?
        .ok_or_else(|| error("Discussion publication lacks its pre-fetch canonical binding"))?;
    if binding.binding != snapshot.target.binding()?
        || binding.reservation_generation != generation
        || !binding.active
    {
        return Err(error(
            "Discussion publication does not match its current bound reservation",
        ));
    }
    let previous_visible = visible_generation(cache, &snapshot.target)?.is_some();
    let mut records = vec![];
    let mut freshly_admitted = BTreeSet::new();
    let mut total = 0usize;
    let mut state = State {
        target: snapshot.target.clone(),
        access: snapshot.access.clone(),
        enumeration_complete: snapshot.enumeration_complete,
        complete: snapshot.complete && snapshot.enumeration_complete,
        reasons: snapshot.reasons.clone(),
        fetched_at: snapshot.fetched_at,
        head_sha: snapshot.head_sha.clone(),
        suppressed_resources: BTreeSet::new(),
        suppressed_threads: BTreeSet::new(),
    };
    let mut budget_interrupted = false;
    for node in &snapshot.excluded_nodes {
        opaque(node)?;
        state
            .suppressed_resources
            .insert(resource(&snapshot.target, node));
    }
    for node in &snapshot.excluded_threads {
        opaque(node)?;
        if snapshot.target.category != DiscussionKind::InlineComment {
            return Err(error(
                "Excluded thread groups belong only to inline sources",
            ));
        }
        state.suppressed_threads.insert(node.clone());
    }
    let mut documents = snapshot.documents()?.into_iter();
    while let Some(document) = documents.next() {
        if cancel.is_cancelled() {
            return Err(error("Discussion hydration cancelled before publication"));
        }
        let payload = serde_json::to_value(&document).map_err(error)?;
        let size = serde_json::to_vec(&payload).map_err(error)?.len();
        if size > limits.max_document_bytes {
            state
                .suppressed_resources
                .insert(document.key.resource.clone());
            state.complete = false;
            state.reasons.push(format!(
                "Oversized discussion {} excluded by explicit document budget",
                document.key.resource
            ));
            continue;
        }
        if records.len() >= limits.max_documents
            || total.saturating_add(size) > limits.max_total_bytes
        {
            budget_interrupted = true;
            state
                .suppressed_resources
                .insert(document.key.resource.clone());
            state
                .suppressed_resources
                .extend(documents.map(|document| document.key.resource));
            state.enumeration_complete = false;
            state.complete = false;
            state
                .reasons
                .push("Discussion document/byte budget reached; loaded subset only".into());
            break;
        }
        total += size;
        let mut record = Record::new(
            DOCUMENT,
            document.key.stable_id(),
            &snapshot.target.repository,
            payload,
        );
        record.fetched_at = snapshot.fetched_at;
        record.source_updated_at = document.metadata.updated_at.clone();
        record.complete = document.metadata.complete;
        freshly_admitted.insert(document.key.resource.clone());
        records.push(record);
    }
    // Suppression records causal knowledge: an unobserved later page is not proof that
    // an earlier known-obsolete body became current again. Read only this source's
    // metadata here, never old document bodies ahead of the footprint preflight.
    let previous_visibility =
        if !state.enumeration_complete && !matches!(snapshot.access, Access::Revoked(_)) {
            read_state(cache, &snapshot.target)?
        } else {
            None
        };
    if let Some(previous) = &previous_visibility {
        state.suppressed_resources.extend(
            previous
                .suppressed_resources
                .difference(&freshly_admitted)
                .cloned(),
        );
        if snapshot.access != Access::Allowed {
            state
                .suppressed_threads
                .extend(previous.suppressed_threads.iter().cloned());
        }
    }
    let mut authoritative = match &snapshot.access {
        Access::Revoked(reason) => {
            state.complete = false;
            state.enumeration_complete = true;
            state.reasons.push(format!(
                "Access revoked; prior private discussion hidden: {reason}"
            ));
            true
        }
        Access::Unavailable(reason) => {
            state.complete = false;
            state.enumeration_complete = false;
            state.reasons.push(format!(
                "Access unavailable; prior private discussion hidden: {reason}"
            ));
            false
        }
        Access::Allowed => state.enumeration_complete,
    };
    if snapshot.access == Access::Allowed && !state.enumeration_complete {
        // Never deserialize an unbounded old frame just to find out it exceeds our budget.
        let (count, bytes) = cache
            .sync_footprint(&snapshot.target.scope, &snapshot.target.source())
            .map_err(error)?;
        if count > limits.max_documents.saturating_add(1) || bytes > limits.max_total_bytes as u64 {
            return unavailable_budget(cache, snapshot, &state, generation, limits, cancel,
                "Existing membership exceeds the selected source budget; old bodies retained but hidden without deserialization");
        }
        let thread_pruning = !budget_interrupted
            && snapshot.thread_enumeration.as_ref().is_some_and(|threads| {
                threads.outer_complete || !threads.completed_threads.is_empty()
            });
        authoritative = thread_pruning;
        let fresh_ids: BTreeSet<_> = records.iter().map(|record| record.id.clone()).collect();
        if let Some(previous) = cache
            .sync_frame(&snapshot.target.scope, &snapshot.target.source())
            .map_err(error)?
        {
            for old in previous
                .records
                .into_iter()
                .filter(|record| record.kind == DOCUMENT)
            {
                if cancel.is_cancelled() {
                    return Err(error("Discussion hydration cancelled before publication"));
                }
                if fresh_ids.contains(&old.id) {
                    continue;
                }
                let document: Document =
                    serde_json::from_value(old.payload.clone()).map_err(error)?;
                validate_cached_document(&document, &old, &snapshot.target)?;
                let hidden_by_binding = !previous_visible;
                if hidden_by_binding {
                    // A capped replacement page proves nothing about an older,
                    // unobserved body after an edit/delete/permission/removal fence.
                    // Retain its bytes, but only exact fresh admission can revive it.
                    state
                        .suppressed_resources
                        .insert(document.key.resource.clone());
                }
                if previous_visibility.as_ref().is_some_and(|previous| {
                    document
                        .metadata
                        .discussion
                        .as_ref()
                        .and_then(|context| context.thread_id.as_ref())
                        .is_some_and(|thread| previous.suppressed_threads.contains(thread))
                }) {
                    // Convert an older whole-group exclusion to exact retained-member
                    // exclusions. Newly observed members were handled above and stay
                    // visible, while unobserved old siblings cannot resurrect.
                    state
                        .suppressed_resources
                        .insert(document.key.resource.clone());
                }
                // Known absence must hide a deleted body even when an adapter budget
                // interruption prevents physically pruning the previous membership.
                if snapshot
                    .thread_enumeration
                    .as_ref()
                    .is_some_and(|enumeration| {
                        document
                            .metadata
                            .discussion
                            .as_ref()
                            .and_then(|context| context.thread_id.as_ref())
                            .is_some_and(|thread| {
                                enumeration.completed_threads.contains(thread)
                                    || enumeration.outer_complete
                                        && !enumeration.observed_threads.contains(thread)
                            })
                    })
                {
                    state
                        .suppressed_resources
                        .insert(document.key.resource.clone());
                }
                if thread_pruning {
                    let context = document
                        .metadata
                        .discussion
                        .as_ref()
                        .expect("Context validated");
                    let thread = context
                        .thread_id
                        .as_ref()
                        .ok_or_else(|| error("Cached inline record lacks canonical thread"))?;
                    let enumeration = snapshot
                        .thread_enumeration
                        .as_ref()
                        .expect("Pruning evidence checked");
                    if (!hidden_by_binding
                        && state.suppressed_resources.contains(&document.key.resource))
                        || state.suppressed_threads.contains(thread)
                        || enumeration.completed_threads.contains(thread)
                        || enumeration.outer_complete
                            && !enumeration.observed_threads.contains(thread)
                    {
                        continue;
                    }
                }
                let bytes = serde_json::to_vec(&old.payload).map_err(error)?.len();
                if records.len() >= limits.max_documents
                    || total.saturating_add(bytes) > limits.max_total_bytes
                {
                    return unavailable_budget(cache, snapshot, &state, generation, limits, cancel,
                        "Fresh plus retained unknown membership exceeds the source budget; no guessed subset or unknown-group pruning was published");
                }
                total += bytes;
                records.push(old); // Preserve original fetched_at/source_updated_at and payload.
            }
        }
        state.complete = false;
        state.reasons.push("Incomplete enumeration retains unobserved groups at their original source/fetch times; retained bodies are stale/partial, not freshly hydrated".into());
    }
    state.reasons.sort();
    state.reasons.dedup();
    let record = state_record(&state)?;
    if snapshot.access == Access::Allowed
        && total.saturating_add(serde_json::to_vec(&record.payload).map_err(error)?.len())
            > limits.max_total_bytes
    {
        return unavailable_budget(cache, snapshot, &state, generation, limits, cancel,
            "Fresh membership and canonical coverage metadata exceed the source budget; bodies retained but hidden");
    }
    records.push(record);
    if cancel.is_cancelled() {
        return Err(error("Discussion hydration cancelled before publication"));
    }
    cache
        .commit_reserved_batch(
            &snapshot.target.scope,
            ReservedSyncBatch {
                source: &snapshot.target.source(),
                generation,
                records: &records,
                complete: state.complete,
                deletion_authoritative: authoritative,
            },
        )
        .map_err(error)
}

fn unavailable_budget(
    cache: &mut Cache,
    snapshot: &DiscussionSnapshot,
    visibility: &State,
    generation: u64,
    limits: &DiscussionLimits,
    cancel: &CancellationToken,
    reason: &str,
) -> Result<bool, QueryError> {
    let mut state = State {
        target: snapshot.target.clone(),
        access: Access::Unavailable(reason.into()),
        enumeration_complete: false,
        complete: false,
        reasons: vec![reason.into()],
        fetched_at: snapshot.fetched_at,
        head_sha: snapshot.head_sha.clone(),
        suppressed_resources: visibility.suppressed_resources.clone(),
        suppressed_threads: visibility.suppressed_threads.clone(),
    };
    if let Some(previous) = read_state(cache, &snapshot.target)? {
        // A budget failure stores no fresh body. Keep every prior/current exclusion
        // so deliberate cap recovery cannot unlock a known superseded old payload.
        state
            .suppressed_resources
            .extend(previous.suppressed_resources);
        state.suppressed_threads.extend(previous.suppressed_threads);
    }
    let record = state_record(&state)?;
    if serde_json::to_vec(&record.payload).map_err(error)?.len() > limits.max_total_bytes {
        invalidate(cache, &snapshot.target)?;
        return Err(error("Source budget cannot hold even canonical unavailable metadata; source visibility and in-flight reads fenced, no replacement body published"));
    }
    if cancel.is_cancelled() {
        return Err(error(
            "Discussion hydration cancelled before budget metadata publication",
        ));
    }
    cache
        .commit_reserved_batch(
            &snapshot.target.scope,
            ReservedSyncBatch {
                source: &snapshot.target.source(),
                generation,
                records: std::slice::from_ref(&record),
                complete: false,
                deletion_authoritative: false,
            },
        )
        .map_err(error)
}
fn state_record(state: &State) -> Result<Record, QueryError> {
    let mut record = Record::new(
        STATE,
        state.target.source(),
        &state.target.repository,
        serde_json::to_value(state).map_err(error)?,
    );
    record.fetched_at = state.fetched_at;
    record.complete = state.complete;
    Ok(record)
}
fn read_state(cache: &Cache, target: &DiscussionTarget) -> Result<Option<State>, QueryError> {
    let Some(record) = cache
        .get(&target.scope, STATE, &target.source())
        .map_err(error)?
    else {
        return Ok(None);
    };
    let state: State = serde_json::from_value(record.payload).map_err(error)?;
    if state.target != *target || record.repository != target.repository {
        return Err(error(
            "Cached visibility metadata belongs to another canonical discussion target",
        ));
    }
    Ok(Some(state))
}
/// The controller atomically writes this metadata with its source-reservation fence.
/// It does not erase private body rows, drafts, confirmed projections or receipt evidence.
pub fn unavailable_record(
    target: &DiscussionTarget,
    reason: &str,
    fetched_at: u64,
) -> Result<Record, QueryError> {
    target.validate()?;
    if reason.len() > 16 * 1024 {
        return Err(error(
            "Discussion invalidation reason exceeds metadata budget",
        ));
    }
    state_record(&State {
        target: target.clone(),
        access: Access::Unavailable(reason.into()),
        enumeration_complete: false,
        complete: false,
        reasons: vec![reason.into()],
        fetched_at,
        head_sha: None,
        suppressed_resources: BTreeSet::new(),
        suppressed_threads: BTreeSet::new(),
    })
}

/// Private bytes are admitted only from explicit current source frames with Allowed access.
/// Missing/revoked/unavailable sources never authorize an older derivative body by ID alone.
pub fn collect_cached(
    cache: &Cache,
    scope: Scope,
    targets: &[DiscussionTarget],
    limits: &DiscussionLimits,
    cancel: &CancellationToken,
) -> Result<CollectedDiscussions, QueryError> {
    positive_scope(&scope)?;
    validate_limits(limits)?;
    let mut result = CollectedDiscussions {
        corpus: Corpus {
            scope: scope.clone(),
            documents: vec![],
            coverage: Coverage {
                complete: true,
                deletion_authoritative: true,
                ..Default::default()
            },
        },
        visible_versions: BTreeMap::new(),
    };
    let mut sources = BTreeSet::new();
    let mut admitted = Vec::new();
    let mut bytes = 0usize;
    for target in targets {
        target.validate()?;
        if target.scope != scope {
            return Err(error("Discussion target belongs to another account"));
        }
        if !sources.insert(target.source()) {
            return Err(error("Duplicate discussion source selection"));
        }
        if !result
            .corpus
            .coverage
            .repositories
            .contains(&target.repository)
        {
            result
                .corpus
                .coverage
                .repositories
                .push(target.repository.clone());
        }
        if cancel.is_cancelled() {
            result.corpus.coverage.cancelled = true;
            result.corpus.coverage.complete = false;
            result.corpus.coverage.deletion_authoritative = false;
            break;
        }
        if result.corpus.documents.len() >= limits.max_documents || bytes >= limits.max_total_bytes
        {
            result.corpus.coverage.complete = false;
            result.corpus.coverage.deletion_authoritative = false;
            result.corpus.coverage.reasons.push("Selected cached discussion budget reached before loading another source; unadmitted source bodies are hidden".into());
            break;
        }
        let Some(generation) = visible_generation(cache, target)? else {
            result.corpus.coverage.complete = false;
            result.corpus.coverage.deletion_authoritative = false;
            result.corpus.coverage.unavailable.push(format!(
                "{} #{} {:?}: canonical visibility is unverified or invalidated; hydrate explicitly",
                target.repository, target.number, target.category
            ));
            continue;
        };
        let (count, stored_bytes) = cache
            .sync_footprint(&scope, &target.source())
            .map_err(error)?;
        if count > limits.max_documents.saturating_add(1)
            || stored_bytes > limits.max_total_bytes as u64
        {
            result.corpus.coverage.complete = false;
            result.corpus.coverage.deletion_authoritative = false;
            result.corpus.coverage.unavailable.push(format!(
                "{} #{} {:?}: source footprint exceeds collection budget; no bodies deserialized",
                target.repository, target.number, target.category
            ));
            continue;
        }
        let Some(frame) = cache.sync_frame(&scope, &target.source()).map_err(error)? else {
            result.corpus.coverage.complete = false;
            result.corpus.coverage.reasons.push(format!(
                "{} #{} {:?} has not been hydrated",
                target.repository, target.number, target.category
            ));
            continue;
        };
        if frame.generation != generation || visible_generation(cache, target)? != Some(generation)
        {
            result.corpus.coverage.complete = false;
            result.corpus.coverage.deletion_authoritative = false;
            result
                .corpus
                .coverage
                .unavailable
                .push("Discussion visibility changed while loading; prior bodies hidden".into());
            continue;
        }
        let state = frame
            .records
            .iter()
            .find(|record| record.kind == STATE && record.id == target.source())
            .ok_or_else(|| error("Discussion source frame lacks canonical access metadata"))?;
        let state: State = serde_json::from_value(state.payload.clone()).map_err(error)?;
        if state.target != *target {
            return Err(error(
                "Cached source target differs from explicit canonical selection",
            ));
        }
        result.corpus.coverage.complete &= state.complete;
        result.corpus.coverage.deletion_authoritative &=
            state.enumeration_complete || matches!(state.access, Access::Revoked(_));
        result.corpus.coverage.reasons.extend(state.reasons);
        if state.access != Access::Allowed {
            result.corpus.coverage.complete = false;
            result.corpus.coverage.unavailable.push(format!(
                "{} #{} {:?}",
                target.repository, target.number, target.category
            ));
            continue;
        }
        let first = result.corpus.documents.len();
        for record in frame
            .records
            .iter()
            .filter(|record| record.kind == DOCUMENT)
        {
            if cancel.is_cancelled() {
                result.corpus.coverage.cancelled = true;
                result.corpus.coverage.complete = false;
                result.corpus.coverage.deletion_authoritative = false;
                break;
            }
            let mut document: Document =
                serde_json::from_value(record.payload.clone()).map_err(error)?;
            validate_cached_document(&document, record, target)?;
            if state.suppressed_resources.contains(&document.key.resource) {
                result.corpus.coverage.complete = false;
                result.corpus.coverage.skipped_large += 1;
                continue;
            }
            let context = document
                .metadata
                .discussion
                .as_ref()
                .ok_or_else(|| error("Cached discussion lacks typed provenance"))?;
            if context
                .thread_id
                .as_ref()
                .is_some_and(|id| state.suppressed_threads.contains(id))
            {
                result.corpus.coverage.complete = false;
                result.corpus.coverage.skipped_large += 1;
                continue;
            }
            document.metadata.complete &= state.complete;
            let size = serde_json::to_vec(&document).map_err(error)?.len();
            if size > limits.max_document_bytes {
                result.corpus.coverage.complete = false;
                result.corpus.coverage.skipped_large += 1;
                continue;
            }
            if result.corpus.documents.len() >= limits.max_documents
                || bytes.saturating_add(size) > limits.max_total_bytes
            {
                result.corpus.coverage.complete = false;
                result.corpus.coverage.deletion_authoritative = false;
                result.corpus.coverage.reasons.push("Cached discussion collection budget reached; old index entries are not admitted implicitly".into());
                break;
            }
            bytes += size;
            result
                .visible_versions
                .insert(document.key.stable_id(), document.projection_checksum()?);
            result.corpus.documents.push(document);
        }
        admitted.push((
            target,
            generation,
            result.corpus.documents[first..]
                .iter()
                .map(|document| document.key.stable_id())
                .collect::<BTreeSet<_>>(),
        ));
    }
    // A different cache connection can fence or refresh an earlier source while
    // later sources are parsed. Recheck before authorizing exact lexical versions.
    let mut invalidated = BTreeSet::new();
    for (target, generation, ids) in admitted {
        if visible_generation(cache, target)? != Some(generation) {
            invalidated.extend(ids);
            result.corpus.coverage.complete = false;
            result.corpus.coverage.deletion_authoritative = false;
            result.corpus.coverage.unavailable.push(
                "Discussion visibility changed before current-version authorization; source hidden"
                    .into(),
            );
        }
    }
    if !invalidated.is_empty() {
        result
            .visible_versions
            .retain(|id, _| !invalidated.contains(id));
        result
            .corpus
            .documents
            .retain(|document| !invalidated.contains(&document.key.stable_id()));
    }
    result.corpus.coverage.documents = result.corpus.documents.len();
    result.corpus.coverage.scanned_bytes = bytes;
    Ok(result)
}
fn validate_cached_document(
    document: &Document,
    record: &Record,
    target: &DiscussionTarget,
) -> Result<(), QueryError> {
    document.validate(&target.scope)?;
    let context = document
        .metadata
        .discussion
        .as_ref()
        .ok_or_else(|| error("Cached discussion lacks typed provenance"))?;
    opaque(&context.resource_node)?;
    if document.source != SourceKind::Collaboration
        || context.repository_id != target.repository_id
        || context.parent_node != target.parent_node
        || context.parent_number != target.number
        || context.kind != target.category
        || context.resource_numeric_id == Some(0)
        || document.key.repository != target.repository
        || record.repository != target.repository
        || document.key.revision != "discussion"
        || document.key.kind
            != match target.category {
                DiscussionKind::PullRequestBody => ResultKind::PullRequest,
                DiscussionKind::IssueBody => ResultKind::Issue,
                _ => ResultKind::Comment,
            }
        || document.key.resource != resource(target, &context.resource_node)
        || record.id != document.key.stable_id()
    {
        return Err(error(
            "Cached discussion belongs to another canonical source",
        ));
    }
    let comment =
        matches!(document.key.kind, ResultKind::Comment).then_some(context.resource_node.as_str());
    let correct_destination = match (&target.parent_kind, &document.destination) {
        (
            ParentKind::PullRequest,
            Destination::PullRequest {
                host,
                repository,
                number,
                comment_id,
                thread_id,
            },
        ) => {
            host == &target.scope.host
                && repository == &target.repository
                && number == &target.number
                && comment_id.as_deref() == comment
                && thread_id == &context.thread_id
        }
        (
            ParentKind::Issue,
            Destination::Issue {
                host,
                repository,
                number,
                comment_id,
            },
        ) => {
            host == &target.scope.host
                && repository == &target.repository
                && number == &target.number
                && comment_id.as_deref() == comment
        }
        _ => false,
    };
    if !correct_destination {
        return Err(error(
            "Cached discussion destination differs from canonical parent/resource context",
        ));
    }
    Ok(())
}
fn resource(target: &DiscussionTarget, node: &str) -> String {
    format!(
        "discussion:{:x}",
        Sha256::digest(
            serde_json::to_vec(&(
                target.repository_id,
                target.parent_kind,
                &target.parent_node,
                target.category,
                node,
            ))
            .expect("Canonical resource serializes")
        )
    )
}
// Conversion keeps provider timestamps and identity fields separate; none are guessed.
#[allow(clippy::too_many_arguments)]
fn entry(
    target: &DiscussionTarget,
    node: &str,
    numeric_id: Option<u64>,
    title: String,
    body: String,
    author: Option<Actor>,
    created_at: Option<String>,
    updated_at: Option<String>,
    url: Option<String>,
) -> DiscussionEntry {
    DiscussionEntry {
        node_id: node.into(),
        numeric_id,
        title,
        body,
        author,
        created_at,
        updated_at,
        context: DiscussionContext {
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
        },
    }
}
fn actor(value: &Value) -> Option<Actor> {
    if value.is_null() {
        return None;
    }
    Some(Actor {
        login: value
            .get("login")
            .and_then(Value::as_str)
            .unwrap_or("")
            .into(),
        display_name: string(value, "name"),
        avatar_url: string(value, "avatar_url").or_else(|| string(value, "avatarUrl")),
        actor_type: string(value, "type")
            .or_else(|| string(value, "__typename"))
            .unwrap_or_else(|| "Unknown".into()),
    })
}
fn string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}
fn nonempty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}
fn positive_scope(scope: &Scope) -> Result<(), QueryError> {
    let id = scope
        .login
        .strip_prefix("user-id:")
        .and_then(|id| id.parse::<u64>().ok())
        .filter(|id| *id > 0)
        .ok_or_else(|| error("Discussion cache requires a verified positive numeric account"))?;
    if Scope::github(&scope.host, Some(id)).map_err(error)? != *scope {
        return Err(error("Discussion scope is not canonical"));
    }
    Ok(())
}
fn opaque(value: &str) -> Result<(), QueryError> {
    if value.is_empty() || value.len() > 512 || !value.bytes().all(|byte| byte.is_ascii_graphic()) {
        Err(error(
            "Canonical opaque resource identity is missing or malformed",
        ))
    } else {
        Ok(())
    }
}
fn validate_limits(limits: &DiscussionLimits) -> Result<(), QueryError> {
    if !(1..=10_000).contains(&limits.max_documents)
        || !(1..=4 * 1024 * 1024).contains(&limits.max_document_bytes)
        || !(1..=32 * 1024 * 1024).contains(&limits.max_total_bytes)
    {
        Err(error("Discussion budgets must be positive within 10000 documents, 4MiB per document and 32MiB per source"))
    } else {
        Ok(())
    }
}
fn error(value: impl std::fmt::Display) -> QueryError {
    QueryError(value.to_string())
}
