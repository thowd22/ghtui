//! Initially unexported integration of confirmed receipt facts and text-source fences.
//! No cached body is parsed and no PR projection, draft or receipt is erased.
use crate::{
    pr_write_journal::{
        PrEffectKind, PrPublicationReference, PrWriteJournal, PrWriteMarker, PrWriteTarget,
    },
    providers::gh::{HostIdentity, PrSummary},
    storage::{Cache, Scope, SyncBindingKind, SyncFence, SyncFenceResult, SyncNamespace},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
#[path = "review_repair_selection_v8.rs"]
mod review_repair_v8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrDiscussionImpact {
    scope: Scope,
    repository: String,
    parent_node: String,
    publication_attempt: u64,
    categories: Vec<SyncBindingKind>,
}

/// Worker-only source admission proof. It contains identities/generations, not bodies.
#[derive(Debug)]
pub struct DiscussionReadGuard {
    database: PathBuf,
    generations: Vec<(PrWriteTarget, i64)>,
    label_fences: Vec<crate::pr_metadata_actions::PrMetadataLabelReadFence>,
    pub unavailable_sources: BTreeMap<String, String>,
    pub recovered_sources: Vec<String>,
    review_selections_v8: Vec<review_repair_v8::ReviewRepairSelectionV8>,
    historical_composite_v8: bool,
}
impl DiscussionReadGuard {
    pub fn blocked_sources(&self) -> &BTreeMap<String, String> {
        &self.unavailable_sources
    }
    pub fn status(&self) -> Vec<String> {
        let mut status = self
            .unavailable_sources
            .values()
            .cloned()
            .collect::<Vec<_>>();
        if self.historical_composite_v8 {
            status.push("Original historical composite and newer single-category receipt validated; fresh source hydration remains required".into());
        }
        status
    }
    pub fn recovered_sources(&self) -> &[String] {
        &self.recovered_sources
    }
}

/// Before collecting cached metadata OR reserving a hydration read, recover
/// genuinely confirmed text fences. Pending rows only make the affected source
/// unavailable with an honest label; they never establish invocation/success.
pub fn guard_discussion_reads(
    database: &Path,
    scope: &Scope,
    targets: &[crate::search::collaboration::DiscussionTarget],
) -> Result<DiscussionReadGuard, String> {
    use crate::search::collaboration::ParentKind;
    let principal = scope
        .login
        .strip_prefix("user-id:")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|id| *id > 0)
        .ok_or("Discussion read guard requires a verified positive GitHub principal")?;
    if Scope::github(scope.host.to_ascii_lowercase(), Some(principal))
        .map_err(|error| error.to_string())?
        != *scope
        || scope.host.is_empty()
        || !scope
            .host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".-".contains(&byte))
    {
        return Err("Discussion read guard requires canonical host/principal scope".into());
    }
    if targets.len() > 256 {
        return Err("Discussion read guard supports at most 256 explicitly selected source categories per bounded worker batch".into());
    }
    let mut cache = Cache::open(database).map_err(|error| error.to_string())?;
    let journal = PrWriteJournal::open(database).map_err(|error| error.to_string())?;
    let mut guard = DiscussionReadGuard {
        database: journal.database_path().to_owned(),
        generations: vec![],
        label_fences: vec![],
        unavailable_sources: BTreeMap::new(),
        recovered_sources: vec![],
        review_selections_v8: vec![],
        historical_composite_v8: false,
    };
    let mut seen = BTreeSet::new();
    let mut receipts = BTreeSet::new();
    let mut unresolved_receipts = BTreeSet::new();
    let mut label_reasons = BTreeMap::new();
    for target in targets {
        target.validate().map_err(|error| error.to_string())?;
        if &target.scope != scope {
            return Err(
                "Discussion read target belongs to another verified principal scope".into(),
            );
        }
        if target.parent_kind != ParentKind::PullRequest {
            continue;
        }
        let key = PrWriteTarget::for_source(
            scope,
            &target.repository,
            &target.parent_node,
            target.number,
        )
        .map_err(|error| error.to_string())?;
        let bound = target.binding().map_err(|error| error.to_string())?;
        let parent_key = (
            key.repository().to_ascii_lowercase(),
            key.node_id().to_owned(),
            key.number(),
        );
        if seen.insert(parent_key.clone()) {
            // Labels participate in every category's filters. Repair BEFORE
            // binding/cache/index admission, including discovered/first-source
            // parents without a selected metadata intent.
            let labels =
                crate::pr_metadata_actions::repair_label_filter_reads_for_target(database, &key)?;
            if let Some(reason) = labels.unavailable_reason() {
                label_reasons.insert(parent_key.clone(), reason.to_owned());
            }
            guard.label_fences.push(labels);
            guard.generations.push((
                key.clone(),
                journal
                    .source_generation(&key)
                    .map_err(|error| error.to_string())?,
            ));
        }
        if let Some(reason) = label_reasons.get(&parent_key) {
            guard
                .unavailable_sources
                .insert(target.source(), reason.clone());
        }
        if let Some(marker) = journal
            .latest_source_receipt(&key, bound.kind)
            .map_err(|error| error.to_string())?
        {
            require_source_marker(&key, &marker)?;
            super::review_operation_impact_v3::validate_original(&marker, &journal, &key)?;
            let categories = impact_categories(&marker)?;
            if !super::review_impact_v2::matches_source(&marker, true, bound.kind, &categories)? {
                return Err("Genuine source receipt does not match its exact closed text category; private text remains hidden".into());
            }
            validate_discussion_read_guard(database, &guard)?;
            let plan = review_repair_v8::ReviewRepairSelectionV8::prepare(
                &journal,
                &key,
                &marker,
                &categories,
            )?;
            let repairs = if let Some((selection, repairs)) = plan {
                guard.historical_composite_v8 |= repairs.len() == 2;
                if !guard
                    .review_selections_v8
                    .iter()
                    .any(|old| old.same_selection(&selection))
                {
                    guard.review_selections_v8.push(selection);
                }
                repairs
            } else {
                vec![marker]
            };
            for receipt in repairs {
                // Original database/generation and BOTH exact actual selected
                // refs immediately precede every original atomic effect set.
                validate_discussion_read_guard(database, &guard)?;
                super::review_operation_impact_v3::validate_original(&receipt, &journal, &key)?;
                let categories = impact_categories(&receipt)?;
                if receipts.insert((receipt.attempt_id, categories.clone())) {
                    let result=cache.fence_resource_sync_once(scope,&SyncFence{namespace:SyncNamespace::Collaboration,repository_id:None,repository:key.repository().to_ascii_lowercase(),resource:key.node_id().into(),kinds:categories},receipt.attempt_id as u64).map_err(|error|format!("Confirmed text visibility recovery failed; private metadata remains hidden, do not resend: {error}"))?;
                    guard.recovered_sources.extend(result.sources);
                }
            }
        }
        if let Some(marker) = journal
            .source_blocking(&key)
            .map_err(|error| error.to_string())?
        {
            require_source_marker(&key, &marker)?;
            super::review_operation_impact_v3::validate_original(&marker, &journal, &key)?;
            let possible = possible_text_categories(&marker)?;
            if super::review_impact_v2::matches_source(&marker, false, bound.kind, &possible)? {
                // A conservative read fence is not a claim of invocation/effect.
                if unresolved_receipts.insert((marker.attempt_id, possible.clone())) {
                    cache
                        .fence_resource_sync(
                            scope,
                            &SyncFence {
                                namespace: SyncNamespace::Collaboration,
                                repository_id: Some(target.repository_id),
                                repository: target.repository.to_ascii_lowercase(),
                                resource: target.parent_node.clone(),
                                kinds: possible,
                            },
                        )
                        .map_err(|error| {
                            format!("Unresolved source visibility is unavailable: {error}")
                        })?;
                }
                guard.unavailable_sources.insert(target.source(),format!("Unresolved PR publication ({:?}); this marker proves neither invocation nor success. Cached {:?} unavailable until explicit inspection/acknowledgement and fresh hydration.",marker.state,bound.kind));
            }
        }
    }
    guard.recovered_sources.sort();
    guard.recovered_sources.dedup();
    validate_discussion_read_guard(database, &guard)?;
    Ok(guard)
}

/// Recheck immediately before exposing/publishing prepared private results.
/// Journal generations also catch failed-cache-fence races and account-scoped
/// publication between body collection and derived index/result admission.
pub fn validate_discussion_read_guard(
    database: &Path,
    guard: &DiscussionReadGuard,
) -> Result<(), String> {
    let resolved = std::fs::canonicalize(database).map_err(|error| error.to_string())?;
    if resolved != guard.database {
        return Err("Discussion read proof belongs to another database".into());
    }
    let journal = PrWriteJournal::open(database).map_err(|error| error.to_string())?;
    for (target, expected) in &guard.generations {
        if journal
            .source_generation(target)
            .map_err(|error| error.to_string())?
            != *expected
        {
            return Err("PR publication changed during metadata collection; private results remain hidden, repeat the read rather than resend a write".into());
        }
    }
    for fence in &guard.label_fences {
        fence.validate()?;
    }
    for selection in &guard.review_selections_v8 {
        selection.validate(database, &journal)?;
    }
    Ok(())
}

fn require_source_marker(target: &PrWriteTarget, marker: &PrWriteMarker) -> Result<(), String> {
    if marker.attempt_id <= 0
        || marker.target.scope() != target.scope()
        || marker.target.node_id() != target.node_id()
        || marker.target.number() != target.number()
        || !marker
            .target
            .repository()
            .eq_ignore_ascii_case(target.repository())
    {
        return Err("Journal source metadata differs from the exact original principal/PR repository/node/number".into());
    }
    Ok(())
}
impl PrDiscussionImpact {
    pub fn categories(&self) -> &[SyncBindingKind] {
        &self.categories
    }
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub fn parent_node(&self) -> &str {
        &self.parent_node
    }
    pub fn repository(&self) -> &str {
        &self.repository
    }
    pub fn publication_attempt(&self) -> u64 {
        self.publication_attempt
    }
}

/// Accept an exact original receipt even when no fresh PR view could be loaded.
/// The original canonical node, account, repository and number are still mandatory.
pub fn confirmed_discussion_impact(
    database: &Path,
    identity: &HostIdentity,
    target: &PrSummary,
    publication: &PrPublicationReference,
) -> Result<PrDiscussionImpact, String> {
    let key = PrWriteTarget::new(identity, target).map_err(|error| error.to_string())?;
    let journal = PrWriteJournal::open(database).map_err(|error| error.to_string())?;
    journal
        .reject_non_state_cache_reference_v74(&key, publication)
        .map_err(|e| e.to_string())?;
    let marker = journal
        .confirmed_publication(&key, publication)
        .map_err(|error| error.to_string())?
        .ok_or("No genuine confirmed original receipt authorizes text-source invalidation")?;
    if marker.target.number() != key.number()
        || !marker
            .target
            .repository()
            .eq_ignore_ascii_case(key.repository())
    {
        return Err(
            "Confirmed text invalidation differs from the exact original PR repository/number"
                .into(),
        );
    }
    journal
        .reject_non_state_cache_repair_v74(&marker)
        .map_err(|e| e.to_string())?;
    let categories = impact_categories(&marker)?;
    super::review_operation_impact_v3::validate_original(&marker, &journal, &key)?;
    Ok(PrDiscussionImpact {
        scope: key.scope().clone(),
        repository: marker.target.repository().to_ascii_lowercase(),
        parent_node: key.node_id().into(),
        publication_attempt: marker.attempt_id as u64,
        categories,
    })
}

/// Call for a confirmed text effect BEFORE attempting a derived projection, also
/// on ConfirmedButRefreshFailed. Reaction/subscription/base changes are no-ops.
/// A storage failure is a cache-visibility failure, never permission to resend.
pub fn invalidate_confirmed_discussions(
    database: &Path,
    identity: &HostIdentity,
    target: &PrSummary,
    publication: &PrPublicationReference,
) -> Result<Option<SyncFenceResult>, String> {
    let impact = confirmed_discussion_impact(database, identity, target, publication)?;
    if impact.categories.is_empty() {
        return Ok(None);
    }
    let mut cache = Cache::open(database).map_err(|error| error.to_string())?;
    cache
        .fence_resource_sync_once(
            &impact.scope,
            &SyncFence {
                namespace: SyncNamespace::Collaboration,
                // The receipt has no repository database ID. Match only positive IDs
                // already registered to this exact original canonical node/name.
                repository_id: None,
                repository: impact.repository,
                resource: impact.parent_node,
                kinds: impact.categories,
            },
            impact.publication_attempt,
        )
        .map(Some)
        .map_err(|error| error.to_string())
}

/// A caller that actually attempted publication may conservatively hide POSSIBLE
/// changed text. A pending marker alone is not proof that invocation occurred.
/// No remote effect, receipt, acknowledgement or resend permission is inferred.
/// Unlike a confirmed fence, this never consumes the receipt ordinal: a later
/// genuine confirmation must still invalidate any intervening fresh hydration.
pub fn invalidate_unresolved_discussions(
    database: &Path,
    identity: &HostIdentity,
    target: &PrSummary,
    publication: &PrPublicationReference,
) -> Result<Option<SyncFenceResult>, String> {
    let key = PrWriteTarget::new(identity, target).map_err(|error| error.to_string())?;
    let journal = PrWriteJournal::open(database).map_err(|error| error.to_string())?;
    journal
        .reject_non_state_cache_reference_v74(&key, publication)
        .map_err(|e| e.to_string())?;
    let marker = journal
        .blocking(&key)
        .map_err(|error| error.to_string())?
        .filter(|marker| {
            marker.publication_reference() == *publication
                && marker.target.number() == key.number()
                && marker
                    .target
                    .repository()
                    .eq_ignore_ascii_case(key.repository())
        })
        .ok_or("No exact original unresolved publication authorizes a possible-text fence")?;
    journal
        .reject_non_state_cache_repair_v74(&marker)
        .map_err(|e| e.to_string())?;
    let categories = possible_text_categories(&marker)?;
    super::review_operation_impact_v3::validate_original(&marker, &journal, &key)?;
    if categories.is_empty() {
        return Ok(None);
    }
    Cache::open(database)
        .map_err(|error| error.to_string())?
        .fence_resource_sync(
            key.scope(),
            &SyncFence {
                namespace: SyncNamespace::Collaboration,
                repository_id: None,
                repository: marker.target.repository().to_ascii_lowercase(),
                resource: key.node_id().into(),
                kinds: categories,
            },
        )
        .map(Some)
        .map_err(|error| error.to_string())
}

fn comment_category(marker: &PrWriteMarker) -> Result<SyncBindingKind, String> {
    if marker.command == "inline-comment" {
        let digest = marker.selected_resource.strip_prefix("inline/")
            .filter(|digest| digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()))
            .ok_or("Inline publication lacks its exact canonical anchor SHA256; no category is guessed")?;
        debug_assert_eq!(digest.len(), 64);
        return Ok(SyncBindingKind::InlineComment);
    }
    if marker.command == "new-comment" && marker.selected_resource == "pr" {
        return Ok(SyncBindingKind::ConversationComment);
    }
    if let Some(id) = marker.selected_resource.strip_prefix("Conversation/") {
        positive_selected_id(id)?;
        return Ok(SyncBindingKind::ConversationComment);
    }
    if let Some(id) = marker.selected_resource.strip_prefix("Review/") {
        positive_selected_id(id)?;
        return Ok(SyncBindingKind::InlineComment);
    }
    Err("Comment publication lacks its exact typed Conversation/Review selection; no category is guessed".into())
}

fn possible_text_categories(marker: &PrWriteMarker) -> Result<Vec<SyncBindingKind>, String> {
    if let Some(categories) = super::issue_link_impact_v87::categories(marker, false)? {
        return Ok(categories);
    }
    if let Some(categories) = super::metadata_members_impact_v48::categories(marker, false)? {
        return Ok(categories);
    }
    if let Some(categories) = super::metadata_base_impact_v32::categories(marker, false)? {
        return Ok(categories);
    }
    if let Some(categories) = super::metadata_text_impact_v25::categories(marker, false)? {
        return Ok(categories);
    }
    if let Some(categories) = super::file_comment_impact_v75::categories(marker, false)? {
        return Ok(categories);
    }
    if let Some(categories) = super::conversation_impact_v17::categories(marker, false)? {
        return Ok(categories);
    }
    if let Some(categories) = super::review_rerequest_impact_v3::categories(marker, false)? {
        return Ok(categories);
    }
    if let Some(categories) = super::thread_reaction_impact_v3::categories(marker, false)? {
        return Ok(categories);
    }
    if let Some(categories) = super::thread_comment_impact_v2::categories(marker, false)? {
        return Ok(categories);
    }
    if let Some(categories) = super::thread_impact_v1::categories(marker, false)? {
        return Ok(categories);
    }
    if let Some(categories) = super::review_impact_v2::categories(marker, false)? {
        return Ok(categories);
    }
    let category = match marker.command.as_str() {
        "new-comment" | "inline-comment" | "edit-comment" | "delete-comment" => {
            Some(comment_category(marker)?)
        }
        "legacy-approve" | "legacy-request-changes" | "legacy-review-comment" => {
            Some(SyncBindingKind::Review)
        }
        "legacy-title" | "legacy-description" => Some(SyncBindingKind::PullRequestBody),
        "add-reaction" | "remove-reaction" | "subscription" | "legacy-base" | "mark-viewed"
        | "unmark-viewed" => None,
        "metadata-add-label"
        | "metadata-remove-label"
        | "metadata-add-assignee"
        | "metadata-remove-assignee"
        | "metadata-add-reviewer"
        | "metadata-remove-reviewer"
        | "metadata-add-team"
        | "metadata-remove-team"
        | "metadata-set-milestone"
        | "metadata-clear-milestone" => None,
        // Structured filter freshness is handled by the exact metadata hook.
        _ => {
            return Err("This unresolved PR command has no declared discussion-text impact".into())
        }
    };
    Ok(category.into_iter().collect())
}

/// Only execution paths that called their publication adapter use this helper.
pub(crate) fn unresolved_visibility_diagnostic(
    journal: &PrWriteJournal,
    identity: &HostIdentity,
    target: &PrSummary,
    publication: &PrPublicationReference,
) -> String {
    invalidate_unresolved_discussions(journal.database_path(), identity, target, publication)
        .err()
        .map_or(String::new(), |error| {
            format!("\nPossible-text cache visibility could not be fenced: {error}. Private search results must stay hidden; do not resend.")
        })
}

fn impact_categories(marker: &PrWriteMarker) -> Result<Vec<SyncBindingKind>, String> {
    if let Some(categories) = super::issue_link_impact_v87::categories(marker, true)? {
        return Ok(categories);
    }
    if let Some(categories) = super::metadata_members_impact_v48::categories(marker, true)? {
        return Ok(categories);
    }
    if let Some(categories) = super::metadata_base_impact_v32::categories(marker, true)? {
        return Ok(categories);
    }
    if let Some(categories) = super::metadata_text_impact_v25::categories(marker, true)? {
        return Ok(categories);
    }
    if let Some(categories) = super::file_comment_impact_v75::categories(marker, true)? {
        return Ok(categories);
    }
    if let Some(categories) = super::conversation_impact_v17::categories(marker, true)? {
        return Ok(categories);
    }
    if let Some(categories) = super::review_rerequest_impact_v3::categories(marker, true)? {
        return Ok(categories);
    }
    if let Some(categories) = super::thread_reaction_impact_v3::categories(marker, true)? {
        return Ok(categories);
    }
    if let Some(categories) = super::thread_comment_impact_v2::categories(marker, true)? {
        return Ok(categories);
    }
    if let Some(categories) = super::thread_impact_v1::categories(marker, true)? {
        return Ok(categories);
    }
    if let Some(categories) = super::review_impact_v2::categories(marker, true)? {
        return Ok(categories);
    }
    let receipt = marker
        .confirmed
        .as_ref()
        .ok_or("Confirmed receipt is missing")?;
    let category=match receipt.kind {
        PrEffectKind::Comment | PrEffectKind::CommentDeleted=>Some(comment_category(marker)?),
        PrEffectKind::Review | PrEffectKind::ReviewDiscarded=>Some(SyncBindingKind::Review),
        PrEffectKind::Metadata=>match marker.command.as_str() {
            "legacy-title" | "legacy-description"=>Some(SyncBindingKind::PullRequestBody),
            "legacy-base"=>None,
            "metadata-add-label" | "metadata-remove-label" | "metadata-add-assignee"
            | "metadata-remove-assignee" | "metadata-add-reviewer" | "metadata-remove-reviewer"
            | "metadata-add-team" | "metadata-remove-team" | "metadata-set-milestone"
            | "metadata-clear-milestone"=>{crate::pr_metadata_actions::validate_recorded_marker(marker)?;None},
            _=>return Err("Metadata receipt needs an explicit changed-field text impact; no whole-repository invalidation is guessed".into()),
        },
        // A merge changes PR state, not comment/review/body text.
        PrEffectKind::Reaction | PrEffectKind::ReactionRemoved | PrEffectKind::Subscription | PrEffectKind::ViewedFile | PrEffectKind::Merge=>None,
        // These are not the currently integrated text forms. Their future
        // controllers must declare any metadata/filter impact explicitly.
        _=>return Err("This PR effect has no implemented discussion-text impact contract".into()),
    };
    Ok(category.into_iter().collect())
}
fn positive_selected_id(value: &str) -> Result<(), String> {
    if value.parse::<u64>().ok().is_none_or(|id| id == 0) {
        return Err("Selected comment identity is invalid".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "discussion_read_guard_tests.rs"]
mod read_guard_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        pr_write_journal::{PrConfirmedEffect, PrWriteIntent},
        storage::{Record, SyncBinding},
    };
    use serde_json::json;
    use std::{collections::BTreeMap, fs, os::unix::fs::PermissionsExt};
    fn directory() -> tempfile::TempDir {
        tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap()
    }
    fn identity() -> HostIdentity {
        HostIdentity {
            host: "github.invalid".into(),
            login: "octocat".into(),
            account_id: Some(42),
            gh_version: "2.101.0".into(),
        }
    }
    fn summary() -> PrSummary {
        serde_json::from_value(json!({"pr":{"host":"github.invalid","id":"PR_exact_7","repository":"example/repo","number":7,"title":"Original title","body":"Original description","url":"https://github.invalid/example/repo/pull/7","author":{"login":"octocat","display_name":null,"avatar_url":null,"actor_type":"User"},"state":"open","draft":false,"created_at":"t0","updated_at":"t0","queues":[]},"head_sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","base_sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","head_ref":"feature","base_ref":"main","merged":false,"auto_merge":false,"merge_commit_sha":null,"mergeable":true,"mergeable_state":"clean","additions":0,"deletions":0,"changed_files":0})).unwrap()
    }
    fn confirmed(
        path: &Path,
        command: &str,
        selected: &str,
        kind: PrEffectKind,
    ) -> PrPublicationReference {
        let target = PrWriteTarget::new(&identity(), &summary()).unwrap();
        let intent = PrWriteIntent {
            command: command.into(),
            selected_resource: selected.into(),
            observed_head: "a".repeat(40),
            values: BTreeMap::from([("body".into(), "confirmed private text".into())]),
        };
        let mut journal = PrWriteJournal::open(path).unwrap();
        let permit = journal.begin(&target, &intent).unwrap();
        journal.claim(&permit, &target, &intent).unwrap();
        journal
            .mark_confirmed(
                &permit,
                &PrConfirmedEffect {
                    kind,
                    resource_id: "10".into(),
                    node_id: Some("RESOURCE_10".into()),
                },
            )
            .unwrap();
        permit.marker().publication_reference()
    }
    #[test]
    fn collaboration_index_confirmed_effect_maps_only_exact_text_categories() {
        let dir = directory();
        let path = dir.path().join("cache.sqlite3");
        for (command, selected, kind, expected) in [
            (
                "new-comment",
                "pr",
                PrEffectKind::Comment,
                Some(SyncBindingKind::ConversationComment),
            ),
            (
                "edit-comment",
                "Conversation/10",
                PrEffectKind::Comment,
                Some(SyncBindingKind::ConversationComment),
            ),
            (
                "delete-comment",
                "Review/10",
                PrEffectKind::CommentDeleted,
                Some(SyncBindingKind::InlineComment),
            ),
            (
                "inline-comment",
                "inline/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                PrEffectKind::Comment,
                Some(SyncBindingKind::InlineComment),
            ),
            (
                "legacy-review-comment",
                "pr",
                PrEffectKind::Review,
                Some(SyncBindingKind::Review),
            ),
            (
                "legacy-description",
                "pr",
                PrEffectKind::Metadata,
                Some(SyncBindingKind::PullRequestBody),
            ),
            (
                "legacy-title",
                "pr",
                PrEffectKind::Metadata,
                Some(SyncBindingKind::PullRequestBody),
            ),
            ("legacy-base", "pr", PrEffectKind::Metadata, None),
            ("subscription", "pr", PrEffectKind::Subscription, None),
            (
                "add-reaction",
                "Conversation/10",
                PrEffectKind::Reaction,
                None,
            ),
        ] {
            let reference = confirmed(&path, command, selected, kind);
            assert_eq!(
                confirmed_discussion_impact(&path, &identity(), &summary(), &reference)
                    .unwrap()
                    .categories(),
                expected.as_slice()
            );
            if command == "inline-comment" {
                let target = PrWriteTarget::new(&identity(), &summary()).unwrap();
                let marker = PrWriteJournal::open(&path)
                    .unwrap()
                    .latest_source_receipt(&target, SyncBindingKind::InlineComment)
                    .unwrap()
                    .unwrap();
                assert_eq!(marker.command, "inline-comment");
                assert_eq!(marker.selected_resource, selected);
                assert_eq!(marker.publication_reference(), reference);
            }
        }
    }
    #[test]
    fn inline_comments_discussion_category_rejects_fake_ids_or_malformed_anchor_digests() {
        for selected in [
            "Review/0",
            "inline/0",
            "inline/",
            "inline/not-an-anchor",
            "inline/AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "inline/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaG",
        ] {
            let dir = directory();
            let path = dir.path().join("cache.sqlite3");
            let reference = confirmed(&path, "inline-comment", selected, PrEffectKind::Comment);
            assert!(
                confirmed_discussion_impact(&path, &identity(), &summary(), &reference).is_err()
            );
            let target = PrWriteTarget::new(&identity(), &summary()).unwrap();
            assert!(PrWriteJournal::open(&path)
                .unwrap()
                .latest_source_receipt(&target, SyncBindingKind::InlineComment)
                .unwrap()
                .is_none());
        }
    }
    #[test]
    fn inline_comments_unresolved_anchor_hides_only_inline_text_without_clearing_pending() {
        let dir = directory();
        let path = dir.path().join("cache.sqlite3");
        let scope = Scope::github("github.invalid", Some(42)).unwrap();
        let mut cache = Cache::open(&path).unwrap();
        let inline_key = format!("collaboration-v1:{}", "a".repeat(64));
        let conversation_key = format!("collaboration-v1:{}", "b".repeat(64));
        for (key, kind) in [
            (inline_key.as_str(), SyncBindingKind::InlineComment),
            (
                conversation_key.as_str(),
                SyncBindingKind::ConversationComment,
            ),
        ] {
            let bound = SyncBinding {
                namespace: SyncNamespace::Collaboration,
                repository_id: 9,
                repository: "example/repo".into(),
                resource: "PR_exact_7".into(),
                kind,
            };
            let reservation = cache.reserve_resource_sync(&scope, key, &bound).unwrap();
            assert!(cache
                .commit_reserved_sync(&scope, key, reservation, &[], true)
                .unwrap());
        }
        let target = PrWriteTarget::new(&identity(), &summary()).unwrap();
        let intent = PrWriteIntent {
            command: "inline-comment".into(),
            selected_resource: format!("inline/{}", "a".repeat(64)),
            observed_head: "a".repeat(40),
            values: BTreeMap::from([("body".into(), "bounded inline draft".into())]),
        };
        let mut journal = PrWriteJournal::open(&path).unwrap();
        let permit = journal.begin(&target, &intent).unwrap();
        journal.claim(&permit, &target, &intent).unwrap();
        let reference = permit.marker().publication_reference();
        let fence = invalidate_unresolved_discussions(&path, &identity(), &summary(), &reference)
            .unwrap()
            .unwrap();
        assert_eq!(fence.sources, vec![inline_key.clone()]);
        assert!(cache
            .binding_status(&scope, &inline_key)
            .unwrap()
            .unwrap()
            .visible_generation
            .is_none());
        assert!(cache
            .binding_status(&scope, &conversation_key)
            .unwrap()
            .unwrap()
            .visible_generation
            .is_some());
        assert_eq!(
            journal
                .blocking(&target)
                .unwrap()
                .unwrap()
                .publication_reference(),
            reference
        );
    }
    #[test]
    fn collaboration_index_invalidation_requires_original_confirmed_account_node_repo_and_attempt()
    {
        let dir = directory();
        let path = dir.path().join("cache.sqlite3");
        let reference = confirmed(&path, "new-comment", "pr", PrEffectKind::Comment);
        let mut other = identity();
        other.account_id = Some(43);
        assert!(confirmed_discussion_impact(&path, &other, &summary(), &reference).is_err());
        for change in ["repo", "number", "node"] {
            let mut changed = summary();
            match change {
                "repo" => changed.pr.repository = "other/repo".into(),
                "number" => changed.pr.number = 8,
                _ => changed.pr.id = "PR_other".into(),
            };
            assert!(confirmed_discussion_impact(&path, &identity(), &changed, &reference).is_err());
        }
        let target = PrWriteTarget::new(&identity(), &summary()).unwrap();
        let intent = PrWriteIntent {
            command: "new-comment".into(),
            selected_resource: "pr".into(),
            observed_head: "a".repeat(40),
            values: BTreeMap::new(),
        };
        let mut journal = PrWriteJournal::open(&path).unwrap();
        let pending = journal.begin(&target, &intent).unwrap();
        assert!(
            confirmed_discussion_impact(
                &path,
                &identity(),
                &summary(),
                &pending.marker().publication_reference()
            )
            .is_err(),
            "a pending-before-send marker is not a confirmed mutation"
        );
    }
    #[test]
    fn collaboration_index_confirmed_text_fences_first_fetch_and_preserves_projection_draft_receipt(
    ) {
        let dir = directory();
        let path = dir.path().join("cache.sqlite3");
        let key = format!("collaboration-v1:{}", "a".repeat(64));
        let scope = Scope::github("github.invalid", Some(42)).unwrap();
        let mut cache = Cache::open(&path).unwrap();
        let token = cache
            .reserve_resource_sync(
                &scope,
                &key,
                &SyncBinding {
                    namespace: SyncNamespace::Collaboration,
                    repository_id: 9,
                    repository: "example/repo".into(),
                    resource: "PR_exact_7".into(),
                    kind: SyncBindingKind::ConversationComment,
                },
            )
            .unwrap();
        let fresh = Record::new(
            "pr-details",
            "PR_exact_7",
            "example/repo",
            json!({"body":"fresh confirmed PR snapshot"}),
        );
        cache.put(&scope, &fresh).unwrap();
        cache
            .put_draft(&scope, "other/pr", "separate saved draft")
            .unwrap();
        let reference = confirmed(&path, "new-comment", "pr", PrEffectKind::Comment);
        let result = invalidate_confirmed_discussions(&path, &identity(), &summary(), &reference)
            .unwrap()
            .unwrap();
        assert_eq!(result.sources, vec![key.clone()]);
        assert!(!cache
            .commit_reserved_sync(
                &scope,
                &key,
                token,
                &[Record::new(
                    "collaboration-document-v1",
                    "old",
                    "example/repo",
                    json!({"body":"old stale private text"})
                )],
                true
            )
            .unwrap());
        assert_eq!(
            cache
                .get(&scope, "pr-details", "PR_exact_7")
                .unwrap()
                .unwrap(),
            fresh
        );
        assert_eq!(
            cache.draft(&scope, "other/pr").unwrap().as_deref(),
            Some("separate saved draft")
        );
        assert!(PrWriteJournal::open(&path)
            .unwrap()
            .confirmed_publication(
                &PrWriteTarget::new(&identity(), &summary()).unwrap(),
                &reference
            )
            .unwrap()
            .is_some());
    }
    #[test]
    fn collaboration_index_reaction_and_subscription_do_not_cancel_text_hydration() {
        let dir = directory();
        let path = dir.path().join("cache.sqlite3");
        let key = format!("collaboration-v1:{}", "b".repeat(64));
        let scope = Scope::github("github.invalid", Some(42)).unwrap();
        let mut cache = Cache::open(&path).unwrap();
        let token = cache
            .reserve_resource_sync(
                &scope,
                &key,
                &SyncBinding {
                    namespace: SyncNamespace::Collaboration,
                    repository_id: 9,
                    repository: "example/repo".into(),
                    resource: "PR_exact_7".into(),
                    kind: SyncBindingKind::ConversationComment,
                },
            )
            .unwrap();
        for (command, kind) in [
            ("add-reaction", PrEffectKind::Reaction),
            ("subscription", PrEffectKind::Subscription),
        ] {
            let reference = confirmed(&path, command, "pr", kind);
            assert!(
                invalidate_confirmed_discussions(&path, &identity(), &summary(), &reference)
                    .unwrap()
                    .is_none()
            );
            assert_eq!(cache.sync_reservation(&scope, &key).unwrap(), Some(token));
        }
    }

    #[test]
    fn collaboration_index_unresolved_possible_text_fence_preserves_receipt_ordinal_for_later_confirmation(
    ) {
        let dir = directory();
        let path = dir.path().join("cache.sqlite3");
        let key = format!("collaboration-v1:{}", "c".repeat(64));
        let scope = Scope::github("github.invalid", Some(42)).unwrap();
        let bound = SyncBinding {
            namespace: SyncNamespace::Collaboration,
            repository_id: 9,
            repository: "example/repo".into(),
            resource: "PR_exact_7".into(),
            kind: SyncBindingKind::ConversationComment,
        };
        let mut cache = Cache::open(&path).unwrap();
        let first = cache.reserve_resource_sync(&scope, &key, &bound).unwrap();
        let record = Record::new(
            "collaboration-document-v1",
            "comment",
            "example/repo",
            json!({"body":"prior cached body"}),
        );
        assert!(cache
            .commit_reserved_sync(&scope, &key, first, std::slice::from_ref(&record), true)
            .unwrap());
        let target = PrWriteTarget::new(&identity(), &summary()).unwrap();
        let intent = PrWriteIntent {
            command: "new-comment".into(),
            selected_resource: "pr".into(),
            observed_head: "a".repeat(40),
            values: BTreeMap::from([("body".into(), "possible changed text".into())]),
        };
        let mut journal = PrWriteJournal::open(&path).unwrap();
        let permit = journal.begin(&target, &intent).unwrap();
        journal.claim(&permit, &target, &intent).unwrap();
        let reference = permit.marker().publication_reference();
        assert_eq!(
            invalidate_unresolved_discussions(&path, &identity(), &summary(), &reference)
                .unwrap()
                .unwrap()
                .sources,
            vec![key.clone()]
        );
        assert_eq!(
            cache
                .binding_status(&scope, &key)
                .unwrap()
                .unwrap()
                .visible_generation,
            None
        );
        assert!(
            journal.blocking(&target).unwrap().is_some(),
            "hiding possible text does not acknowledge or confirm publication"
        );
        let intervening = cache.reserve_resource_sync(&scope, &key, &bound).unwrap();
        assert!(cache
            .commit_reserved_sync(
                &scope,
                &key,
                intervening,
                std::slice::from_ref(&record),
                true
            )
            .unwrap());
        journal
            .mark_confirmed(
                &permit,
                &PrConfirmedEffect {
                    kind: PrEffectKind::Comment,
                    resource_id: "10".into(),
                    node_id: Some("IC_10".into()),
                },
            )
            .unwrap();
        assert!(
            invalidate_unresolved_discussions(&path, &identity(), &summary(), &reference).is_err(),
            "confirmed markers cannot be interpreted as unknown effects"
        );
        let confirmed =
            invalidate_confirmed_discussions(&path, &identity(), &summary(), &reference)
                .unwrap()
                .unwrap();
        assert_eq!(
            confirmed.sources,
            vec![key.clone()],
            "unknown fence cannot consume later genuine receipt authority"
        );
        assert_eq!(
            cache
                .binding_status(&scope, &key)
                .unwrap()
                .unwrap()
                .visible_generation,
            None
        );
    }

    #[test]
    fn collaboration_index_unresolved_fence_requires_positive_principal_and_exact_original_reference(
    ) {
        let dir = directory();
        let path = dir.path().join("cache.sqlite3");
        let target = PrWriteTarget::new(&identity(), &summary()).unwrap();
        let intent = PrWriteIntent {
            command: "edit-comment".into(),
            selected_resource: "Conversation/10".into(),
            observed_head: "a".repeat(40),
            values: BTreeMap::new(),
        };
        let mut journal = PrWriteJournal::open(&path).unwrap();
        let permit = journal.begin(&target, &intent).unwrap();
        let reference = permit.marker().publication_reference();
        for account in [None, Some(0), Some(43)] {
            let mut other = identity();
            other.account_id = account;
            assert!(
                invalidate_unresolved_discussions(&path, &other, &summary(), &reference).is_err()
            );
        }
        for field in ["attempt", "node", "head", "digest"] {
            let mut wrong = reference.clone();
            match field {
                "attempt" => wrong.attempt_id += 1,
                "node" => wrong.pr_node_id = "PR_other".into(),
                "head" => wrong.observed_head = "f".repeat(40),
                _ => wrong.payload_sha256 = "f".repeat(64),
            };
            assert!(
                invalidate_unresolved_discussions(&path, &identity(), &summary(), &wrong).is_err(),
                "{field}"
            );
        }
        for field in ["repository", "number", "node"] {
            let mut wrong = summary();
            match field {
                "repository" => wrong.pr.repository = "other/repo".into(),
                "number" => wrong.pr.number += 1,
                _ => wrong.pr.id = "PR_other".into(),
            };
            assert!(
                invalidate_unresolved_discussions(&path, &identity(), &wrong, &reference).is_err(),
                "{field}"
            );
        }
        assert!(journal.blocking(&target).unwrap().is_some());
    }

    #[test]
    fn collaboration_index_unresolved_nontext_and_unknown_selectors_never_guess_changed_category() {
        let dir = directory();
        for (command, selected, expected) in [
            ("add-reaction", "Conversation/10", Some(false)),
            ("subscription", "pr", Some(false)),
            ("legacy-base", "pr", Some(false)),
            ("mark-viewed", "file/fixture", Some(false)),
            ("unmark-viewed", "file/fixture", Some(false)),
            ("delete-comment", "Review/10", Some(true)),
            ("edit-comment", "Review/0", None),
            ("new-unimplemented-operation", "pr", None),
        ] {
            let path = dir.path().join(format!("{command}.sqlite3"));
            let target = PrWriteTarget::new(&identity(), &summary()).unwrap();
            let intent = PrWriteIntent {
                command: command.into(),
                selected_resource: selected.into(),
                observed_head: "a".repeat(40),
                values: BTreeMap::new(),
            };
            let mut journal = PrWriteJournal::open(&path).unwrap();
            let permit = journal.begin(&target, &intent).unwrap();
            let result = invalidate_unresolved_discussions(
                &path,
                &identity(),
                &summary(),
                &permit.marker().publication_reference(),
            );
            match expected {
                Some(has_text) => assert_eq!(result.unwrap().is_some(), has_text, "{command}"),
                None => assert!(result.is_err(), "{command}"),
            };
            assert!(journal.blocking(&target).unwrap().is_some());
        }
    }
}
