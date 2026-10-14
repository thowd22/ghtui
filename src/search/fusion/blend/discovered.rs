//! Staged worker-only original-receipt and canonical metadata attachment.
use super::*;
use crate::{
    pr_projection::{guard_discussion_reads, validate_discussion_read_guard, DiscussionReadGuard},
    search::collaboration::{BindingMetadataVisibility, ParentKind},
    storage::{ResourceBindingSnapshot, SyncBindingKind},
};
use std::path::{Path, PathBuf};

const PARENTS: usize = 1000;
const BINDINGS: usize = 5000;
const STATE_BYTES: usize = 32 * 1024 * 1024;

pub(super) struct DiscoveredVisibility {
    database: PathBuf,
    parents: BTreeMap<ParentAlias, DiscussionTarget>,
    snapshots: Vec<ResourceBindingSnapshot>,
    guards: Vec<DiscussionReadGuard>,
}
impl DiscoveredVisibility {
    pub(super) fn target(&self, alias: &ParentAlias) -> Option<&DiscussionTarget> {
        self.parents.get(alias)
    }
    pub(super) fn validate(&self, cache: &Cache) -> Result<(), QueryError> {
        if cache.binding_lookup_issuer().map_err(error)? != self.database {
            return Err(error(
                "Discovered visibility belongs to another cache issuer",
            ));
        }
        for guard in &self.guards {
            validate_discussion_read_guard(&self.database, guard).map_err(error)?;
        }
        for snapshot in &self.snapshots {
            snapshot.validate(cache).map_err(error)?;
        }
        Ok(())
    }
    /// Numeric/missing provenance is not made safe by an empty cached lookup.
    /// Root must enrich it to a genuinely verified opaque parent before attach.
    pub(super) fn accepts(
        &self,
        hit: &SearchHit,
        identity: &RemoteIdentity,
    ) -> Result<bool, QueryError> {
        let kind = hit.document.key.kind;
        if !matches!(kind, ResultKind::PullRequest | ResultKind::Issue) {
            return Ok(true);
        }
        let RemoteIdentity::OpaqueNode {
            node_id,
            repository_id,
            ..
        } = identity
        else {
            return Ok(false);
        };
        let alias = ParentAlias::new(
            &hit.document.key.scope,
            &hit.document.key.repository,
            kind,
            node_id,
        )?;
        let Some(target) = self.parents.get(&alias) else {
            return Ok(false);
        };
        let number = match &hit.document.destination {
            Destination::PullRequest { number, .. } | Destination::Issue { number, .. } => *number,
            _ => {
                return Err(error(
                    "Verified parent has no exact GitHub parent destination",
                ))
            }
        };
        if number != target.number || repository_id.is_some_and(|id| id != target.repository_id) {
            return Err(error(
                "Verified discovered parent repository/number contradicts actual remote provenance",
            ));
        }
        Ok(true)
    }
}

impl CurrentVisibility {
    /// Worker-only proof check for a retained CurrentCanonical Open. A parent
    /// body proof cannot authorize a comment/thread or an edited projection.
    /// None means this snapshot does not authorize the supplied document.
    pub fn validate_document(
        &self,
        cache: &Cache,
        document: &Document,
    ) -> Result<Option<DiscussionTarget>, QueryError> {
        self.validate(cache)?;
        document.validate(&self.scope)?;
        if document.source != SourceKind::Collaboration {
            return Ok(None);
        }
        let Some(parent) = body_alias(document)? else {
            return Ok(None);
        };
        if self.blocked_parents.contains(&parent) {
            return Err(error(
                "Current canonical body is visibility-fenced or unverified",
            ));
        }
        let Some(current) = self.parents.get(&parent) else {
            return Ok(None);
        };
        let checksum = document.projection_checksum()?;
        if current.key != document.key
            || current.projection_checksum()? != checksum
            || self.versions.get(&document.key.stable_id()) != Some(&checksum)
        {
            return Err(error("Current canonical Open projection differs from its retained exact identity/checksum"));
        }
        let target = self
            .discovered
            .as_ref()
            .and_then(|proof| proof.target(&parent))
            .or_else(|| {
                self.epochs.iter().find_map(|(target, _)| {
                    (is_body(target) && alias(target).ok().as_ref() == Some(&parent))
                        .then_some(target)
                })
            });
        let Some(target) = target else {
            return Ok(None);
        };
        let discussion = document
            .metadata
            .discussion
            .as_ref()
            .ok_or_else(|| error("Canonical body Open lacks typed context"))?;
        if discussion.repository_id != target.repository_id
            || discussion.parent_node != target.parent_node
            || discussion.parent_number != target.number
            || discussion.kind != target.category
        {
            return Err(error(
                "Current canonical Open target contradicts the genuine retained body identity",
            ));
        }
        self.validate(cache)?;
        Ok(Some(target.clone()))
    }

    /// Root's live worker must call this even with no verified parent targets.
    /// Targets are genuine current provider/enrichment identities, not values
    /// reconstructed from display IDs. Root brackets all work with fresh current
    /// principal checks; this low-level cache/journal proof is not authentication.
    pub fn attach_discovered(
        &mut self,
        database: &Path,
        cache: &Cache,
        context: &BlendContext,
        parent_bodies: &[DiscussionTarget],
        limits: &DiscussionLimits,
        cancel: &CancellationToken,
    ) -> Result<(), QueryError> {
        context.validate()?;
        positive_scope(&context.scope)?;
        if !context.policy.enabled || self.scope != context.scope || self.discovered.is_some() {
            return Err(error(
                "Discovered visibility requires the same fresh un-attached account context",
            ));
        }
        if parent_bodies.len() > PARENTS {
            return Err(error("Discovered parent identity budget exceeds1000"));
        }
        let database = std::fs::canonicalize(database).map_err(error)?;
        if cache.binding_lookup_issuer().map_err(error)? != database {
            return Err(error(
                "Original-receipt guard and body cache have different issuers",
            ));
        }
        let mut proof = DiscoveredVisibility {
            database,
            parents: BTreeMap::new(),
            snapshots: vec![],
            guards: vec![],
        };
        let mut expanded = Vec::new();
        let mut groups = BTreeMap::<String, Vec<ParentAlias>>::new();
        for body in parent_bodies {
            body.validate()?;
            if body.scope != context.scope
                || !contains_repository(&context.remote_repositories, &body.repository)
                || !is_body(body)
            {
                return Err(error("Discovered parent is not an exact body identity inside the declared account/remote repository scope"));
            }
            let alias = alias(body)?;
            if let Some(existing) = proof.parents.get(&alias) {
                if existing != body {
                    return Err(error(
                        "Duplicate discovered node has contradictory parent identity",
                    ));
                }
                continue;
            }
            groups
                .entry(body.repository.to_ascii_lowercase())
                .or_default()
                .push(alias.clone());
            expanded.extend(categories(body));
            proof.parents.insert(alias, body.clone());
        }
        let mut blocked = BTreeSet::new();
        // Repair actual confirmed SQL failures and gate unresolved original
        // publications BEFORE admitting metadata or deserializing any body.
        for batch in expanded.chunks(256) {
            if cancel.is_cancelled() {
                return Err(error(
                    "Discovered visibility cancelled before original-receipt inspection",
                ));
            }
            let guard =
                guard_discussion_reads(&proof.database, &context.scope, batch).map_err(error)?;
            for target in batch {
                if guard.blocked_sources().contains_key(&target.source()) {
                    blocked.insert(alias(target)?);
                }
            }
            proof.guards.push(guard);
        }
        let mut remaining_rows = BINDINGS;
        let mut remaining_bytes = STATE_BYTES;
        let mut allowed_bodies = Vec::new();
        for (repository, aliases) in groups {
            if cancel.is_cancelled() {
                return Err(error(
                    "Discovered visibility cancelled during metadata admission",
                ));
            }
            if remaining_rows == 0 || remaining_bytes == 0 {
                blocked.extend(aliases);
                continue;
            }
            let nodes = aliases
                .iter()
                .map(|alias| alias.node.clone())
                .collect::<Vec<_>>();
            let snapshot = cache
                .lookup_resource_bindings(
                    &context.scope,
                    &repository,
                    Some(&nodes),
                    &[
                        SyncBindingKind::PullRequestBody,
                        SyncBindingKind::IssueBody,
                        SyncBindingKind::ConversationComment,
                        SyncBindingKind::Review,
                        SyncBindingKind::InlineComment,
                    ],
                    remaining_rows,
                )
                .map_err(error)?;
            remaining_rows =
                remaining_rows.saturating_sub(snapshot.rows().len() + snapshot.tombstones().len());
            let bytes = snapshot
                .rows()
                .iter()
                .filter_map(|row| row.state.payload.as_ref())
                .map(String::len)
                .sum::<usize>();
            let fits_bytes = bytes <= remaining_bytes;
            remaining_bytes = remaining_bytes.saturating_sub(bytes);
            if !snapshot.complete() || !fits_bytes {
                blocked.extend(aliases.iter().cloned());
                proof.snapshots.push(snapshot);
                continue;
            }
            for parent in &aliases {
                let body = &proof.parents[parent];
                let required_body = body.binding()?.kind;
                let mut canonical_body = None;
                let mut known = false;
                let mut current_categories = BTreeSet::new();
                for row in snapshot
                    .rows()
                    .iter()
                    .filter(|row| row.status.binding.resource == parent.node)
                {
                    known = true;
                    match collaboration::binding_metadata_visibility(&context.scope, row)? {
                        BindingMetadataVisibility::CurrentAllowed { target, .. } => {
                            same_parent(body, &target)?;
                            current_categories.insert(row.status.binding.kind);
                            if row.status.binding.kind == required_body {
                                canonical_body = Some(target);
                            }
                        }
                        BindingMetadataVisibility::Hidden { target, .. } => {
                            if let Some(target) = target {
                                same_parent(body, &target)?;
                            }
                            blocked.insert(parent.clone());
                        }
                    }
                }
                if known && !current_categories.contains(&required_body) {
                    blocked.insert(parent.clone());
                }
                for tombstone in snapshot
                    .tombstones()
                    .iter()
                    .filter(|tombstone| tombstone.resource == parent.node)
                {
                    if !current_categories.contains(&tombstone.kind) {
                        blocked.insert(parent.clone());
                    }
                }
                if current_categories.contains(&required_body) && !blocked.contains(parent) {
                    // Identity matching is case-insensitive for repo names, but
                    // cached STATE carries its exact authenticated spelling.
                    // Load that decoded canonical target, never reconstruct it.
                    allowed_bodies.push(canonical_body.ok_or_else(|| {
                        error("Current body metadata has no authenticated canonical target")
                    })?);
                }
            }
            proof.snapshots.push(snapshot);
        }
        // Bodies are loaded only for exact metadata-authorized targets after
        // original-receipt guards. Missing document rows cannot authorize remote
        // indexed parent fragments when canonical state claims a known source.
        let collected = collaboration::collect_cached(
            cache,
            context.scope.clone(),
            &allowed_bodies,
            limits,
            cancel,
        )?;
        let complete = collected.corpus.coverage.complete
            && !collected.corpus.coverage.cancelled
            && collected.corpus.coverage.unavailable.is_empty();
        let mut documents = BTreeMap::new();
        if complete {
            for mut document in collected.corpus.documents {
                if let Some(sets) = context
                    .repository_sets
                    .get(&document.key.repository.to_ascii_lowercase())
                {
                    document.metadata.repository_sets = sets.clone();
                }
                let Some(parent) = body_alias(&document)? else {
                    return Err(error(
                        "Discovered body collection returned a non-parent projection",
                    ));
                };
                if !proof.parents.contains_key(&parent)
                    || documents.insert(parent, document).is_some()
                {
                    return Err(error("Discovered body projection has duplicate or unrequested canonical identity"));
                }
            }
        }
        for body in &allowed_bodies {
            let parent = alias(body)?;
            if !complete || !documents.contains_key(&parent) {
                blocked.insert(parent);
            }
        }
        proof.validate(cache)?;
        let mut prepared = Vec::new();
        for (parent, document) in documents {
            if blocked.contains(&parent) {
                continue;
            }
            let checksum = document.projection_checksum()?;
            if let Some(existing) = self.parents.get(&parent) {
                if existing.projection_checksum()? != checksum {
                    return Err(error(
                        "Selected and discovered current canonical body projections disagree",
                    ));
                }
            }
            prepared.push((parent, document, checksum));
        }
        let mut prepared_epochs = Vec::new();
        for target in expanded {
            let generation = bound_generation(cache, &target)?;
            if !self.epochs.iter().any(|(existing, _)| existing == &target) {
                prepared_epochs.push((target, generation));
            }
        }
        for (parent, document, checksum) in prepared {
            self.versions.insert(document.key.stable_id(), checksum);
            self.parents.insert(parent, Arc::new(document));
        }
        self.blocked_parents.extend(blocked);
        self.epochs.extend(prepared_epochs);
        self.discovered = Some(proof);
        self.validate(cache)?;
        Ok(())
    }
}

impl std::fmt::Debug for CurrentVisibility {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CurrentVisibility")
            .field("scope", &self.scope)
            .field("version_count", &self.versions.len())
            .field("parent_count", &self.parents.len())
            .field("blocked_parent_count", &self.blocked_parents.len())
            .field("epoch_count", &self.epochs.len())
            .field("discovered_attached", &self.discovered.is_some())
            .finish_non_exhaustive()
    }
}

fn is_body(target: &DiscussionTarget) -> bool {
    matches!(
        (target.parent_kind, target.category),
        (ParentKind::PullRequest, DiscussionKind::PullRequestBody)
            | (ParentKind::Issue, DiscussionKind::IssueBody)
    )
}
fn alias(target: &DiscussionTarget) -> Result<ParentAlias, QueryError> {
    ParentAlias::new(
        &target.scope,
        &target.repository,
        match target.parent_kind {
            ParentKind::PullRequest => ResultKind::PullRequest,
            ParentKind::Issue => ResultKind::Issue,
        },
        &target.parent_node,
    )
}
fn categories(body: &DiscussionTarget) -> Vec<DiscussionTarget> {
    let categories: &[DiscussionKind] = match body.parent_kind {
        ParentKind::PullRequest => &[
            DiscussionKind::PullRequestBody,
            DiscussionKind::ConversationComment,
            DiscussionKind::Review,
            DiscussionKind::InlineComment,
        ],
        ParentKind::Issue => &[
            DiscussionKind::IssueBody,
            DiscussionKind::ConversationComment,
        ],
    };
    categories
        .iter()
        .map(|category| DiscussionTarget {
            category: *category,
            ..body.clone()
        })
        .collect()
}
fn same_parent(body: &DiscussionTarget, state: &DiscussionTarget) -> Result<(), QueryError> {
    if body.scope != state.scope
        || body.repository_id != state.repository_id
        || !body.repository.eq_ignore_ascii_case(&state.repository)
        || body.parent_kind != state.parent_kind
        || body.parent_node != state.parent_node
        || body.number != state.number
    {
        return Err(error("Canonical metadata contradicts the genuine discovered account/repository/node/type/number; no stale body or guessed alias admitted"));
    }
    Ok(())
}
