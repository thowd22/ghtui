//! Meaningful source-generation, canonical projection and original Git-message contracts.
#[path = "git_support/mod.rs"]
mod git_support;
use ghtui::{
    providers::{
        gh::{
            Actor, ConversationComment, Coverage, Permissions, PrDetails, PrSummary, PullRequest,
            Repository, ReviewThread,
        },
        runner::CancellationToken,
    },
    search::{
        collaboration::{self, Access, DiscussionLimits, DiscussionSnapshot, ThreadInput},
        commits::{self, CommitLimits, CommitSelection},
        hydration,
        model::{DiscussionKind, ResultKind},
        scan::exact_search,
        Query, ScanLimits, SearchMode,
    },
    storage::{Cache, Scope},
};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

fn scope() -> Scope {
    Scope::github("github.invalid", Some(42)).unwrap()
}
fn repository() -> Repository {
    Repository {
        host: "github.invalid".into(),
        id: 101,
        full_name: "org/app".into(),
        description: "fixture".into(),
        default_branch: "main".into(),
        private: true,
        archived: false,
        permissions: Permissions {
            pull: true,
            ..Default::default()
        },
    }
}
fn details(body: &str) -> PrDetails {
    PrDetails {
        summary: PrSummary {
            pr: PullRequest {
                host: "github.invalid".into(),
                id: "PR_parent".into(),
                repository: "org/app".into(),
                number: 7,
                title: "Discussion fixture".into(),
                body: "PR body only".into(),
                url: "https://github.invalid/org/app/pull/7".into(),
                author: Actor {
                    login: "writer".into(),
                    actor_type: "User".into(),
                    ..Default::default()
                },
                state: "open".into(),
                draft: false,
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-02T00:00:00Z".into(),
                queues: BTreeSet::new(),
            },
            head_sha: "a".repeat(40),
            base_sha: "b".repeat(40),
            head_ref: "topic".into(),
            base_ref: "main".into(),
            merged: false,
            auto_merge: false,
            merge_commit_sha: None,
            mergeable: Some(true),
            mergeable_state: "clean".into(),
            additions: 0,
            deletions: 0,
            changed_files: 4000,
        },
        comments: vec![ConversationComment {
            id: 12,
            node_id: "IC_stable".into(),
            body: body.into(),
            author: Actor {
                login: "alice".into(),
                actor_type: "Unknown".into(),
                ..Default::default()
            },
            created_at: "2026-01-03T00:00:00Z".into(),
            updated_at: "2026-01-04T00:00:00Z".into(),
            url: "https://github.invalid/org/app/pull/7#issuecomment-12".into(),
            card_v115: None,
        }],
        commits: vec![],
        files: vec![],
        reviews: vec![
            json!({"id":13,"node_id":"PRR_stable","body":"review-only phrase","state":"APPROVED","user":{"login":"reviewer"},"submitted_at":"2026-01-05T00:00:00Z","commit_id":"a".repeat(40)}),
        ],
        timeline: vec![],
        coverage: Coverage {
            complete: false,
            reasons: vec!["file cap".into()],
            ..Default::default()
        },
    }
}
fn comment(body: &str, fetched_at: u64) -> DiscussionSnapshot {
    DiscussionSnapshot::from_pr(scope(), &repository(), &details(body), None, fetched_at)
        .unwrap()
        .into_iter()
        .find(|source| source.target.category == DiscussionKind::ConversationComment)
        .unwrap()
}
fn publish(cache: &mut Cache, snapshot: &DiscussionSnapshot) -> u64 {
    let generation = collaboration::reserve(cache, &snapshot.target).unwrap();
    assert!(collaboration::publish(
        cache,
        snapshot,
        generation,
        &DiscussionLimits::default(),
        &CancellationToken::default()
    )
    .unwrap());
    generation
}

#[test]
fn collaboration_index_bound_first_fetch_source_remove_fences_late_hydration_and_rank_without_erasing_intent(
) {
    let mut cache = Cache::memory().unwrap();
    let snapshot = comment("not yet published", 1);
    cache
        .put_draft(&scope(), "composer", "retained user draft")
        .unwrap();
    let slow = collaboration::reserve(&mut cache, &snapshot.target).unwrap();
    let rank = cache
        .reserve_sync_generation(&scope(), "search-index:pending")
        .unwrap();
    let fenced = collaboration::invalidate(&mut cache, &snapshot.target).unwrap();
    assert_eq!(fenced.sources, [snapshot.target.source()]);
    assert_eq!(fenced.active_reservations_fenced, 2);
    assert!(!collaboration::publish(
        &mut cache,
        &snapshot,
        slow,
        &DiscussionLimits::default(),
        &CancellationToken::default()
    )
    .unwrap());
    assert!(!cache
        .commit_reserved_sync(&scope(), "search-index:pending", rank, &[], true)
        .unwrap());
    assert!(cache
        .sync_checkpoint(&scope(), &snapshot.target.source())
        .unwrap()
        .is_none());
    let collected = collaboration::collect_cached(
        &cache,
        scope(),
        std::slice::from_ref(&snapshot.target),
        &DiscussionLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(collected.corpus.documents.is_empty() && collected.visible_versions.is_empty());
    assert_eq!(
        cache.draft(&scope(), "composer").unwrap().as_deref(),
        Some("retained user draft")
    );
}

#[test]
fn collaboration_index_bound_fence_partial_rehydrate_cannot_resurrect_unknown_old_body_even_after_rereserve(
) {
    let mut cache = Cache::memory().unwrap();
    let mut old = comment("old first body", 1);
    let mut second = old.entries[0].clone();
    second.node_id = "IC_second".into();
    second.numeric_id = Some(14);
    second.context.resource_node.clone_from(&second.node_id);
    second.context.resource_numeric_id = second.numeric_id;
    second.body = "old unknown second body".into();
    old.entries.push(second);
    publish(&mut cache, &old);
    let old_second_id = old.documents().unwrap()[1].key.stable_id();
    collaboration::invalidate(&mut cache, &old.target).unwrap();
    let mut current = comment("fresh first body", 2);
    current.enumeration_complete = false;
    current.complete = false;
    let fresh = collaboration::reserve(&mut cache, &current.target).unwrap();
    let hidden = collaboration::collect_cached(
        &cache,
        scope(),
        &[old.target.clone()],
        &DiscussionLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(
        hidden.corpus.documents.is_empty(),
        "A new read reservation is not a source publication"
    );
    assert!(collaboration::publish(
        &mut cache,
        &current,
        fresh,
        &DiscussionLimits::default(),
        &CancellationToken::default()
    )
    .unwrap());
    let selected = collaboration::collect_cached(
        &cache,
        scope(),
        &[old.target.clone()],
        &DiscussionLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(selected.corpus.documents.len(), 1);
    assert_eq!(selected.corpus.documents[0].body, "fresh first body");
    assert!(!selected.visible_versions.contains_key(&old_second_id));
    assert_eq!(
        cache
            .get(&scope(), "collaboration-document-v1", &old_second_id)
            .unwrap()
            .unwrap()
            .payload["body"],
        "old unknown second body",
        "Unobserved old bytes are retained, not guessed deleted"
    );
    current.entries.clear();
    current.fetched_at = 3;
    publish(&mut cache, &current);
    let still_partial = collaboration::collect_cached(
        &cache,
        scope(),
        &[old.target.clone()],
        &DiscussionLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(still_partial.corpus.documents.len(), 1);
    assert_eq!(still_partial.corpus.documents[0].body, "fresh first body");
    assert!(!still_partial.visible_versions.contains_key(&old_second_id));
    let mut observed_second = old.entries[1].clone();
    observed_second.body = "fresh second recovery".into();
    current.entries.push(observed_second);
    current.fetched_at = 4;
    publish(&mut cache, &current);
    let recovered = collaboration::collect_cached(
        &cache,
        scope(),
        &[old.target],
        &DiscussionLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(recovered.corpus.documents.len(), 2);
    assert!(recovered
        .corpus
        .documents
        .iter()
        .any(|document| document.body == "fresh second recovery"));
    assert!(!recovered.corpus.coverage.complete);
}

#[test]
fn collaboration_index_bound_visibility_uses_published_generation_not_conservative_active_read_flag(
) {
    let mut cache = Cache::memory().unwrap();
    let sources =
        DiscussionSnapshot::from_pr(scope(), &repository(), &details("comment"), None, 1).unwrap();
    let review = sources
        .iter()
        .find(|source| source.target.category == DiscussionKind::Review)
        .unwrap();
    let comment = sources
        .iter()
        .find(|source| source.target.category == DiscussionKind::ConversationComment)
        .unwrap();
    let review_generation = publish(&mut cache, review);
    publish(&mut cache, comment);
    collaboration::invalidate(&mut cache, &comment.target).unwrap();
    let status = cache
        .binding_status(&scope(), &review.target.source())
        .unwrap()
        .unwrap();
    assert!(!status.active);
    assert_eq!(status.visible_generation, Some(review_generation));
    let selected = collaboration::collect_cached(
        &cache,
        scope(),
        &[review.target.clone(), comment.target.clone()],
        &DiscussionLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(selected.corpus.documents.len(), 1);
    assert_eq!(selected.corpus.documents[0].body, "review-only phrase");
}

#[test]
fn collaboration_index_bound_tiny_metadata_cap_error_fences_source_and_rank_before_returning_preserving_rows(
) {
    let mut cache = Cache::memory().unwrap();
    let old = comment("prior private source body", 1);
    publish(&mut cache, &old);
    let old_id = old.documents().unwrap()[0].key.stable_id();
    let rank = cache
        .reserve_sync_generation(&scope(), "search-index:old-body")
        .unwrap();
    let mut current = comment("unadmitted current edit", 2);
    current.enumeration_complete = false;
    current.complete = false;
    let generation = collaboration::reserve(&mut cache, &current.target).unwrap();
    let tiny = DiscussionLimits {
        max_documents: 1,
        max_document_bytes: 1,
        max_total_bytes: 1,
    };
    let error = collaboration::publish(
        &mut cache,
        &current,
        generation,
        &tiny,
        &CancellationToken::default(),
    )
    .unwrap_err();
    assert!(error.0.contains("visibility and in-flight reads fenced"));
    assert!(cache
        .binding_status(&scope(), &current.target.source())
        .unwrap()
        .unwrap()
        .visible_generation
        .is_none());
    assert!(!cache
        .commit_reserved_sync(&scope(), "search-index:old-body", rank, &[], true)
        .unwrap());
    let selected = collaboration::collect_cached(
        &cache,
        scope(),
        std::slice::from_ref(&old.target),
        &DiscussionLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(selected.corpus.documents.is_empty() && selected.visible_versions.is_empty());
    assert_eq!(
        cache
            .get(&scope(), "collaboration-document-v1", &old_id)
            .unwrap()
            .unwrap()
            .payload["body"],
        "prior private source body"
    );
    // Explicit cap recovery without a fresh observation still cannot revive old text.
    current.entries.clear();
    publish(&mut cache, &current);
    assert!(collaboration::collect_cached(
        &cache,
        scope(),
        &[old.target],
        &DiscussionLimits::default(),
        &CancellationToken::default()
    )
    .unwrap()
    .corpus
    .documents
    .is_empty());
}

#[test]
fn collaboration_index_bound_inline_partial_group_prunes_only_known_absence_and_retains_hidden_unknown_bytes(
) {
    let mut cache = Cache::memory().unwrap();
    let old = inline(
        &[
            thread(
                "PRRT_observed",
                &[
                    ("PRRC_current", "old current"),
                    ("PRRC_deleted", "known deleted"),
                ],
            ),
            thread(
                "PRRT_unknown",
                &[("PRRC_unseen", "unknown retained private body")],
            ),
        ],
        true,
        &["PRRT_observed", "PRRT_unknown"],
        1,
    );
    publish(&mut cache, &old);
    collaboration::invalidate(&mut cache, &old.target).unwrap();
    let current = inline(
        &[thread(
            "PRRT_observed",
            &[("PRRC_current", "fresh current")],
        )],
        false,
        &["PRRT_observed"],
        2,
    );
    publish(&mut cache, &current);
    let frame = cache
        .sync_frame(&scope(), &old.target.source())
        .unwrap()
        .unwrap();
    assert!(!frame.complete);
    assert!(frame
        .records
        .iter()
        .any(|record| record.payload["body"] == "unknown retained private body"));
    assert!(!frame
        .records
        .iter()
        .any(|record| record.payload["body"] == "known deleted"));
    let collected = collaboration::collect_cached(
        &cache,
        scope(),
        &[current.target],
        &DiscussionLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(collected.corpus.documents.len(), 1);
    assert_eq!(collected.corpus.documents[0].body, "fresh current");
    assert_eq!(collected.visible_versions.len(), 1);
}

#[test]
fn collaboration_index_bound_legacy_or_fenced_sources_are_hidden_before_invalid_body_deserialization(
) {
    let mut prepared = Cache::memory().unwrap();
    let source = comment("legacy body", 1);
    publish(&mut prepared, &source);
    let mut records = prepared
        .sync_frame(&scope(), &source.target.source())
        .unwrap()
        .unwrap()
        .records;
    records
        .iter_mut()
        .find(|record| record.kind == "collaboration-document-v1")
        .unwrap()
        .payload = json!({"not a Document":"sentinel"});
    let mut legacy = Cache::memory().unwrap();
    assert!(legacy
        .commit_sync(&scope(), &source.target.source(), 1, &records, true)
        .unwrap());
    let selected = collaboration::collect_cached(
        &legacy,
        scope(),
        std::slice::from_ref(&source.target),
        &DiscussionLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(
        selected.corpus.documents.is_empty() && selected.visible_versions.is_empty(),
        "An unbound migrated source is never parsed as a searchable body"
    );
    let id = source.documents().unwrap()[0].key.stable_id();
    collaboration::invalidate(&mut prepared, &source.target).unwrap();
    let mut poison = prepared
        .get(&scope(), "collaboration-document-v1", &id)
        .unwrap()
        .unwrap();
    poison.payload = json!({"not a Document":"fenced sentinel"});
    poison.fetched_at += 1;
    prepared.put(&scope(), &poison).unwrap();
    let selected = collaboration::collect_cached(
        &prepared,
        scope(),
        &[source.target],
        &DiscussionLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(
        selected.corpus.documents.is_empty() && selected.visible_versions.is_empty(),
        "A fenced source is rejected before any invalid body is parsed"
    );
}

#[test]
fn collaboration_index_resource_identity_survives_head_rewrite_newer_reserved_body_wins_clock_rollback(
) {
    let mut cache = Cache::memory().unwrap();
    let old = comment("old hidden phrase", 100);
    publish(&mut cache, &old);
    let old_doc = old.documents().unwrap().pop().unwrap();
    let slow = collaboration::reserve(&mut cache, &old.target).unwrap();
    let mut current = comment("current edited phrase", 50);
    current.head_sha = Some("c".repeat(40));
    let fresh = publish(&mut cache, &current);
    assert!(fresh > slow);
    assert!(!collaboration::publish(
        &mut cache,
        &old,
        slow,
        &DiscussionLimits::default(),
        &CancellationToken::default()
    )
    .unwrap());
    let selected = collaboration::collect_cached(
        &cache,
        scope(),
        &[current.target.clone()],
        &DiscussionLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(selected.corpus.documents.len(), 1);
    let doc = &selected.corpus.documents[0];
    assert_eq!(doc.body, "current edited phrase");
    assert_eq!(doc.metadata.fetched_at, 50);
    assert_eq!(old_doc.key.stable_id(), doc.key.stable_id());
    assert_eq!(doc.key.revision, "discussion");
    assert_ne!(
        old_doc.projection_checksum().unwrap(),
        doc.projection_checksum().unwrap()
    );
    assert_eq!(
        selected.visible_versions[&doc.key.stable_id()],
        doc.projection_checksum().unwrap()
    );
}

#[test]
fn collaboration_index_deleted_comments_prune_independently_of_file_caps_and_partial_inline_sources(
) {
    let mut cache = Cache::memory().unwrap();
    let old = comment("obsolete phrase", 1);
    publish(&mut cache, &old);
    let sources =
        DiscussionSnapshot::from_pr(scope(), &repository(), &details("obsolete phrase"), None, 2)
            .unwrap();
    let inline = sources
        .iter()
        .find(|source| source.target.category == DiscussionKind::InlineComment)
        .unwrap();
    assert!(!inline.enumeration_complete);
    publish(&mut cache, inline);
    let mut deleted = old.clone();
    deleted.entries.clear();
    deleted.fetched_at = 3;
    assert!(deleted.enumeration_complete);
    publish(&mut cache, &deleted);
    let selected = collaboration::collect_cached(
        &cache,
        scope(),
        &[deleted.target, inline.target.clone()],
        &DiscussionLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(selected.corpus.documents.is_empty());
    assert!(selected.visible_versions.is_empty());
    assert!(!selected.corpus.coverage.complete);
    assert!(!cache
        .sync_frame(&scope(), &old.target.source())
        .unwrap()
        .unwrap()
        .records
        .iter()
        .any(|record| record.kind == "collaboration-document-v1"));
}

#[test]
fn collaboration_index_revocation_and_unknown_access_hide_old_bodies_even_partial_without_removing_drafts(
) {
    let mut cache = Cache::memory().unwrap();
    let old = comment("private body", 1);
    publish(&mut cache, &old);
    cache.put_draft(&scope(), "draft", "user intent").unwrap();
    let mut unavailable = old.clone();
    unavailable.access = Access::Unavailable("identity/permission unavailable".into());
    unavailable.enumeration_complete = false;
    publish(&mut cache, &unavailable);
    let selected = collaboration::collect_cached(
        &cache,
        scope(),
        std::slice::from_ref(&old.target),
        &DiscussionLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(selected.corpus.documents.is_empty() && selected.visible_versions.is_empty());
    assert!(cache
        .sync_frame(&scope(), &old.target.source())
        .unwrap()
        .unwrap()
        .records
        .iter()
        .any(|record| record.kind == "collaboration-document-v1"));
    let mut revoked = old.clone();
    revoked.access = Access::Revoked("HTTP403".into());
    revoked.enumeration_complete = false;
    publish(&mut cache, &revoked);
    let selected = collaboration::collect_cached(
        &cache,
        scope(),
        std::slice::from_ref(&old.target),
        &DiscussionLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(selected.corpus.documents.is_empty() && selected.visible_versions.is_empty());
    assert!(selected.corpus.coverage.deletion_authoritative);
    assert!(!selected.corpus.coverage.complete);
    assert!(!cache
        .sync_frame(&scope(), &old.target.source())
        .unwrap()
        .unwrap()
        .records
        .iter()
        .any(|record| record.kind == "collaboration-document-v1"));
    assert_eq!(
        cache.draft(&scope(), "draft").unwrap().as_deref(),
        Some("user intent")
    );
}

#[test]
fn collaboration_index_clear_while_inflight_rejects_late_completion_and_new_reservation_publishes()
{
    let mut cache = Cache::memory().unwrap();
    let snapshot = comment("private phrase", 1);
    publish(&mut cache, &snapshot);
    let old = collaboration::reserve(&mut cache, &snapshot.target).unwrap();
    cache.clear_records(&scope()).unwrap();
    assert!(!collaboration::publish(
        &mut cache,
        &snapshot,
        old,
        &DiscussionLimits::default(),
        &CancellationToken::default()
    )
    .unwrap());
    let empty = collaboration::collect_cached(
        &cache,
        scope(),
        std::slice::from_ref(&snapshot.target),
        &DiscussionLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(empty.corpus.documents.is_empty() && empty.visible_versions.is_empty());
    let new = publish(&mut cache, &snapshot);
    assert!(new > old);
    let other = Scope::github("github.invalid", Some(43)).unwrap();
    assert!(collaboration::collect_cached(
        &cache,
        other,
        std::slice::from_ref(&snapshot.target),
        &DiscussionLimits::default(),
        &CancellationToken::default()
    )
    .is_err());
}

#[test]
fn collaboration_index_original_full_commit_messages_preserve_git_roles_head_index_and_replacement_notice(
) {
    let fixture = git_support::Fixture::linear();
    let tree = String::from_utf8(git_support::git(&fixture.root, ["write-tree"]).stdout).unwrap();
    let object=format!("tree {}\nparent {}\nauthor alice <writer@example.invalid> 1767225600 +0530\ncommitter Different Integrator <integrator@example.invalid> 1767312000 -0700\n\nSaved title\n\ncommit-only secret phrase\n",tree.trim(),fixture.head());
    let id = String::from_utf8(
        git_support::git_input(
            &fixture.root,
            ["hash-object", "-t", "commit", "-w", "--stdin"],
            object.as_bytes(),
        )
        .stdout,
    )
    .unwrap()
    .trim()
    .to_owned();
    git_support::git(&fixture.root, ["update-ref", "refs/heads/messages", &id]);
    let mut replacement = object.replace("commit-only secret phrase", "replacement-only phrase");
    replacement = replacement.replace("Saved title", "Replacement title");
    let replacement_id = String::from_utf8(
        git_support::git_input(
            &fixture.root,
            ["hash-object", "-t", "commit", "-w", "--stdin"],
            replacement.as_bytes(),
        )
        .stdout,
    )
    .unwrap()
    .trim()
    .to_owned();
    git_support::git(&fixture.root, ["replace", &id, &replacement_id]);
    let head = fixture.head();
    let index = std::fs::read(fixture.root.join(".git/index")).unwrap();
    let selected = commits::collect(
        scope(),
        &[CommitSelection {
            repository: "org/app".into(),
            directory: fixture.root.clone(),
            revision: "messages".into(),
            max_commits: 1,
        }],
        &CommitLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(selected.corpus.documents.len(), 1);
    let document = &selected.corpus.documents[0];
    assert_eq!(document.key.revision, id);
    assert!(document.body.contains("commit-only secret phrase"));
    assert!(!document.body.contains("replacement-only"));
    assert_eq!(document.key.kind, ResultKind::Commit);
    assert!(document.metadata.actors.is_empty());
    assert_eq!(document.metadata.git_author.as_ref().unwrap().name, "alice");
    assert_eq!(
        document.metadata.git_committer.as_ref().unwrap().name,
        "Different Integrator"
    );
    assert_eq!(
        document.metadata.created_at.as_deref(),
        Some("2026-01-01T00:00:00Z")
    );
    assert!(!document.metadata.history_overrides.is_empty());
    assert!(!selected.corpus.coverage.complete);
    let page = exact_search(
        &selected.corpus,
        &Query::parse("commit-only secret phrase type:commit", SearchMode::Exact).unwrap(),
        &ScanLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(page.hits.len(), 1);
    assert_eq!(page.hits[0].destination(), document.destination);
    assert_eq!(fixture.head(), head);
    assert_eq!(
        std::fs::read(fixture.root.join(".git/index")).unwrap(),
        index
    );
}

#[test]
fn collaboration_index_review_author_type_and_source_timestamps_are_not_inferred_from_login_or_fetch_time(
) {
    let snapshots =
        DiscussionSnapshot::from_pr(scope(), &repository(), &details("body"), None, 999).unwrap();
    let reviews = snapshots
        .iter()
        .find(|source| source.target.category == DiscussionKind::Review)
        .unwrap();
    let documents = reviews.documents().unwrap();
    assert_eq!(documents[0].body, "review-only phrase");
    assert_eq!(documents[0].metadata.actors[0].actor_type, "Unknown");
    assert_eq!(documents[0].metadata.updated_at, None);
    assert_eq!(
        documents[0].metadata.created_at.as_deref(),
        Some("2026-01-05T00:00:00Z")
    );
    assert_eq!(documents[0].metadata.fetched_at, 999);
    let parent = snapshots
        .iter()
        .find(|source| source.target.category == DiscussionKind::PullRequestBody)
        .unwrap();
    assert_eq!(
        parent.documents().unwrap()[0]
            .metadata
            .discussion
            .as_ref()
            .unwrap()
            .resource_numeric_id,
        None,
        "PR number is not a REST database identity"
    );
}

fn thread(id: &str, nodes: &[(&str, &str)]) -> ReviewThread {
    ReviewThread {
        id: id.into(), path: "src/日本語.rs".into(), side: "RIGHT".into(), line: Some(12),
        start_line: Some(10), resolved: false, outdated: true,
        viewer_can_resolve: false, viewer_can_unresolve: false, comments_complete: true,
        comments: nodes.iter().map(|(node, body)| json!({
            "id":node,"body":body,"author":{"__typename":"User","login":"alice"},
            "fullDatabaseId":"18446744073709551615", "createdAt":"2026-02-01T00:00:00Z",
            "updatedAt":"2026-02-02T00:00:00Z","url":"https://github.invalid/org/app/pull/7#discussion_r14",
            "commit":{"oid":"a".repeat(40)}
        })).collect(),
    }
}
fn inline(
    threads: &[ReviewThread],
    outer_complete: bool,
    completed: &[&str],
    fetched_at: u64,
) -> DiscussionSnapshot {
    let inner: BTreeMap<_, _> = completed
        .iter()
        .map(|id| ((*id).to_owned(), true))
        .collect();
    DiscussionSnapshot::from_pr(
        scope(),
        &repository(),
        &details("body"),
        Some(ThreadInput {
            threads,
            outer_complete,
            complete: outer_complete && threads.iter().all(|thread| inner.contains_key(&thread.id)),
            comment_enumeration: &inner,
            reasons: &[],
        }),
        fetched_at,
    )
    .unwrap()
    .into_iter()
    .find(|snapshot| snapshot.target.category == DiscussionKind::InlineComment)
    .unwrap()
}

#[test]
fn collaboration_index_inline_requires_outer_and_inner_evidence_and_preserves_bigint_without_truncation(
) {
    let mut threads = vec![thread(
        "PRRT_one",
        &[
            ("PRRC_one", "inline phrase"),
            ("PRRC_two", "another phrase"),
        ],
    )];
    threads[0].comments[1]["fullDatabaseId"] = json!("18446744073709551616");
    let snapshot = inline(&threads, false, &["PRRT_one"], 7);
    assert!(!snapshot.enumeration_complete && !snapshot.complete);
    let documents = snapshot.documents().unwrap();
    assert!(
        documents[0].key.path.is_none(),
        "Path must not destabilize comment resource IDs"
    );
    assert!(documents[0].matches(
        &Query::parse(
            &format!("path:src/日本語.rs rev:{} type:comment", "a".repeat(40)),
            SearchMode::Exact
        )
        .unwrap()
    ));
    assert_eq!(
        documents[0]
            .metadata
            .discussion
            .as_ref()
            .unwrap()
            .resource_numeric_id,
        Some(u64::MAX)
    );
    assert_eq!(
        documents[1]
            .metadata
            .discussion
            .as_ref()
            .unwrap()
            .resource_numeric_id,
        None
    );
    assert_eq!(
        documents[1]
            .metadata
            .discussion
            .as_ref()
            .unwrap()
            .resource_node,
        "PRRC_two"
    );
    assert_eq!(
        documents[0]
            .metadata
            .discussion
            .as_ref()
            .unwrap()
            .path
            .as_ref()
            .unwrap()
            .0,
        "src/日本語.rs".as_bytes()
    );
    let no_inner_evidence = inline(&threads, true, &[], 7);
    assert!(
        !no_inner_evidence.enumeration_complete,
        "legacy comments_complete alone cannot prove current provider page enumeration"
    );
    assert_eq!(
        snapshot.thread_enumeration.unwrap().completed_threads,
        ["PRRT_one".to_owned()].into_iter().collect()
    );
}

#[test]
fn collaboration_index_oversized_observed_edit_never_falls_back_to_older_small_body_when_partial() {
    let mut cache = Cache::memory().unwrap();
    let old = comment("obsolete small phrase", 1);
    publish(&mut cache, &old);
    let mut edited = comment(&"new body ".repeat(4096), 2);
    edited.enumeration_complete = false;
    edited.complete = false;
    let generation = collaboration::reserve(&mut cache, &edited.target).unwrap();
    assert!(collaboration::publish(
        &mut cache,
        &edited,
        generation,
        &DiscussionLimits {
            max_document_bytes: 2 * 1024,
            ..DiscussionLimits::default()
        },
        &CancellationToken::default()
    )
    .unwrap());
    let selected = collaboration::collect_cached(
        &cache,
        scope(),
        &[edited.target],
        &DiscussionLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(selected.corpus.documents.is_empty() && selected.visible_versions.is_empty());
    assert!(!selected.corpus.coverage.complete);
    assert_eq!(selected.corpus.coverage.skipped_large, 1);
    assert!(
        cache
            .sync_frame(&scope(), &old.target.source())
            .unwrap()
            .unwrap()
            .records
            .iter()
            .any(|record| record.kind == "collaboration-document-v1"),
        "retained storage is not authorization to show old bytes"
    );
}

fn provider_snapshot(
    kind: ghtui::providers::gh::DiscussionRecordKind,
) -> ghtui::providers::gh::DiscussionSnapshot {
    use ghtui::providers::gh::{
        DiscussionCoverage, DiscussionParent, DiscussionParentKind, DiscussionRecord, HostIdentity,
    };
    ghtui::providers::gh::DiscussionSnapshot {
        identity: HostIdentity {
            host: "github.invalid".into(),
            login: "operator".into(),
            account_id: Some(42),
            gh_version: "fixture".into(),
        },
        parent: DiscussionParent {
            host: "github.invalid".into(),
            repository_id: 101,
            repository: "org/app".into(),
            node_id: "PR_parent".into(),
            numeric_id: Some(7001),
            number: 7,
            kind: DiscussionParentKind::PullRequest,
            title: "Canonical title".into(),
            body: "canonical body phrase".into(),
            body_complete: true,
            state: "open".into(),
            author: None,
            labels: Some(vec!["bug".into()]),
            created_at: Some("2026-01-01T00:00:00Z".into()),
            updated_at: Some("2026-01-02T00:00:00Z".into()),
            url: "https://github.invalid/org/app/pull/7".into(),
            head_sha: Some("a".repeat(40)),
        },
        records: vec![DiscussionRecord {
            node_id: if kind == ghtui::providers::gh::DiscussionRecordKind::Review {
                "PRR_13"
            } else {
                "IC_12"
            }
            .into(),
            numeric_id: Some(12),
            kind,
            body: "only typed record phrase".into(),
            author: None,
            created_at: None,
            updated_at: None,
            submitted_at: Some("2026-01-03T00:00:00Z".into()),
            url: "https://github.invalid/org/app/pull/7#canonical-record".into(),
            state: Some("APPROVED".into()),
            commit_sha: if kind == ghtui::providers::gh::DiscussionRecordKind::Review {
                Some("b".repeat(40))
            } else {
                None
            },
        }],
        coverage: DiscussionCoverage {
            enumeration_complete: true,
            complete: true,
            loaded: 1,
            pages: 1,
            ..Default::default()
        },
    }
}

#[test]
fn collaboration_index_typed_provider_projection_keeps_actual_parent_id_labels_review_commit_and_nullable_actor(
) {
    use ghtui::providers::gh::DiscussionRecordKind;
    let provider = provider_snapshot(DiscussionRecordKind::Review);
    let snapshots = hydration::from_records(
        scope(),
        &repository(),
        &provider,
        DiscussionKind::Review,
        77,
    )
    .unwrap();
    let parent = snapshots
        .iter()
        .find(|source| source.target.category == DiscussionKind::PullRequestBody)
        .unwrap()
        .documents()
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(
        parent
            .metadata
            .discussion
            .as_ref()
            .unwrap()
            .resource_numeric_id,
        Some(7001)
    );
    assert_eq!(parent.metadata.labels, ["bug"]);
    let review = snapshots
        .iter()
        .find(|source| source.target.category == DiscussionKind::Review)
        .unwrap()
        .documents()
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(
        review
            .metadata
            .discussion
            .as_ref()
            .unwrap()
            .commit_sha
            .as_deref(),
        Some("b".repeat(40).as_str())
    );
    assert!(
        review.metadata.actors.is_empty(),
        "nullable actor is not guessed from operator"
    );
    assert_eq!(
        review.metadata.created_at.as_deref(),
        Some("2026-01-03T00:00:00Z")
    );
    assert_eq!(review.metadata.updated_at, None);
    assert_eq!(
        hydration::pr_targets(scope(), &repository(), &details("body").summary)
            .unwrap()
            .len(),
        4
    );
    let mut another = provider.clone();
    another.parent.repository_id = 999;
    assert!(
        hydration::from_records(scope(), &repository(), &another, DiscussionKind::Review, 77)
            .is_err()
    );
    another = provider.clone();
    another.identity.account_id = Some(43);
    assert!(
        hydration::from_records(scope(), &repository(), &another, DiscussionKind::Review, 77)
            .is_err()
    );
}

#[test]
fn collaboration_index_provider_capped_parent_never_indexes_cleared_placeholder_or_previous_body() {
    use ghtui::providers::gh::DiscussionRecordKind;
    let mut cache = Cache::memory().unwrap();
    let old = provider_snapshot(DiscussionRecordKind::ConversationComment);
    let old_sources = hydration::from_records(
        scope(),
        &repository(),
        &old,
        DiscussionKind::ConversationComment,
        1,
    )
    .unwrap();
    let old_body = &old_sources[0];
    publish(&mut cache, old_body);
    let mut capped = old.clone();
    capped.parent.body.clear();
    capped.parent.body_complete = false;
    capped.parent.labels = None;
    let sources = hydration::from_records(
        scope(),
        &repository(),
        &capped,
        DiscussionKind::ConversationComment,
        2,
    )
    .unwrap();
    let body = &sources[0];
    assert!(body.entries.is_empty() && body.excluded_nodes.contains("PR_parent"));
    assert!(!body.complete && body.enumeration_complete);
    publish(&mut cache, body);
    let current = collaboration::collect_cached(
        &cache,
        scope(),
        std::slice::from_ref(&body.target),
        &DiscussionLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(current.corpus.documents.is_empty() && current.visible_versions.is_empty());
    assert!(current
        .corpus
        .coverage
        .reasons
        .iter()
        .any(|reason| reason.contains("Parent labels unavailable")));
}

#[test]
fn collaboration_index_category_coalescing_rejects_changed_parent_body_head_and_duplicate_category()
{
    use ghtui::providers::gh::DiscussionRecordKind;
    let comments = provider_snapshot(DiscussionRecordKind::ConversationComment);
    let reviews = provider_snapshot(DiscussionRecordKind::Review);
    let a = hydration::from_records(
        scope(),
        &repository(),
        &comments,
        DiscussionKind::ConversationComment,
        10,
    )
    .unwrap();
    let b = hydration::from_records(scope(), &repository(), &reviews, DiscussionKind::Review, 10)
        .unwrap();
    assert_eq!(
        hydration::coalesce(a.clone().into_iter().chain(b).collect())
            .unwrap()
            .len(),
        3
    );
    assert!(hydration::coalesce(a.clone().into_iter().chain(a.clone()).collect()).is_err());
    let mut changed = reviews.clone();
    changed.parent.body = "different parent version".into();
    let b = hydration::from_records(scope(), &repository(), &changed, DiscussionKind::Review, 10)
        .unwrap();
    assert!(hydration::coalesce(a.clone().into_iter().chain(b).collect()).is_err());
    changed = reviews;
    changed.parent.head_sha = Some("c".repeat(40));
    let b = hydration::from_records(scope(), &repository(), &changed, DiscussionKind::Review, 10)
        .unwrap();
    assert!(hydration::coalesce(a.into_iter().chain(b).collect()).is_err());
}

#[test]
fn collaboration_index_inner_complete_group_prunes_deleted_comment_unknown_group_retains_original_times_and_partial_flag(
) {
    let mut cache = Cache::memory().unwrap();
    let old = inline(
        &[
            thread(
                "PRRT_known",
                &[
                    ("PRRC_keep", "old keep"),
                    ("PRRC_deleted", "deleted phrase"),
                ],
            ),
            thread("PRRT_unknown", &[("PRRC_unknown", "retained old phrase")]),
        ],
        true,
        &["PRRT_known", "PRRT_unknown"],
        1,
    );
    publish(&mut cache, &old);
    let current = inline(
        &[
            thread("PRRT_known", &[("PRRC_keep", "fresh keep")]),
            thread("PRRT_unknown", &[]),
        ],
        false,
        &["PRRT_known"],
        2,
    );
    publish(&mut cache, &current);
    let frame = cache
        .sync_frame(&scope(), &current.target.source())
        .unwrap()
        .unwrap();
    assert!(!frame.complete);
    let bodies: Vec<_> = frame
        .records
        .iter()
        .filter(|record| record.kind == "collaboration-document-v1")
        .map(|record| record.payload["body"].as_str().unwrap())
        .collect();
    assert!(bodies.contains(&"fresh keep") && bodies.contains(&"retained old phrase"));
    assert!(!bodies.contains(&"deleted phrase"));
    let retained = frame
        .records
        .iter()
        .find(|record| record.payload["body"] == "retained old phrase")
        .unwrap();
    assert_eq!(retained.fetched_at, 1);
    assert_eq!(
        retained.source_updated_at.as_deref(),
        Some("2026-02-02T00:00:00Z")
    );
    let collected = collaboration::collect_cached(
        &cache,
        scope(),
        &[current.target],
        &DiscussionLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(collected.corpus.documents.len(), 2);
    assert!(!collected.corpus.coverage.complete);
    assert!(collected
        .corpus
        .documents
        .iter()
        .all(|document| !document.metadata.complete));
}

#[test]
fn collaboration_index_complete_outer_catalog_removes_absent_thread_but_retains_incomplete_observed_group(
) {
    let mut cache = Cache::memory().unwrap();
    let old = inline(
        &[
            thread(
                "PRRT_present",
                &[
                    ("PRRC_keep", "current old"),
                    ("PRRC_unseen", "unseen retained"),
                ],
            ),
            thread("PRRT_absent", &[("PRRC_gone", "removed thread phrase")]),
        ],
        true,
        &["PRRT_present", "PRRT_absent"],
        1,
    );
    publish(&mut cache, &old);
    let current = inline(
        &[thread("PRRT_present", &[("PRRC_keep", "current loaded")])],
        true,
        &[],
        2,
    );
    publish(&mut cache, &current);
    let frame = cache
        .sync_frame(&scope(), &current.target.source())
        .unwrap()
        .unwrap();
    assert!(!frame.complete);
    let collected = collaboration::collect_cached(
        &cache,
        scope(),
        &[current.target],
        &DiscussionLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    let bodies: Vec<_> = collected
        .corpus
        .documents
        .iter()
        .map(|document| document.body.as_str())
        .collect();
    assert_eq!(bodies.len(), 2);
    assert!(bodies.contains(&"current loaded") && bodies.contains(&"unseen retained"));
    assert!(!bodies.contains(&"removed thread phrase"));
}

#[test]
fn collaboration_index_bounded_partial_union_fails_closed_preserves_records_draft_and_recovers_only_after_deliberate_larger_cap(
) {
    let mut cache = Cache::memory().unwrap();
    let one = "one ".repeat(128);
    let two = "two ".repeat(128);
    let new = "new ".repeat(128);
    let old = inline(
        &[thread(
            "PRRT_unknown",
            &[("PRRC_old_one", &one), ("PRRC_old_two", &two)],
        )],
        true,
        &["PRRT_unknown"],
        1,
    );
    publish(&mut cache, &old);
    cache.put_draft(&scope(), "keep", "user intent").unwrap();
    let before = cache
        .sync_frame(&scope(), &old.target.source())
        .unwrap()
        .unwrap();
    let (_, bytes) = cache
        .sync_footprint(&scope(), &old.target.source())
        .unwrap();
    let current = inline(&[thread("PRRT_new", &[("PRRC_new", &new)])], false, &[], 2);
    let limits = DiscussionLimits {
        max_total_bytes: bytes as usize + 64,
        ..DiscussionLimits::default()
    };
    let generation = collaboration::reserve(&mut cache, &current.target).unwrap();
    assert!(collaboration::publish(
        &mut cache,
        &current,
        generation,
        &limits,
        &CancellationToken::default()
    )
    .unwrap());
    let hidden = collaboration::collect_cached(
        &cache,
        scope(),
        std::slice::from_ref(&current.target),
        &DiscussionLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(hidden.corpus.documents.is_empty() && hidden.visible_versions.is_empty());
    let unavailable = cache
        .sync_frame(&scope(), &old.target.source())
        .unwrap()
        .unwrap();
    assert!(!unavailable.complete);
    let old_rows: Vec<_> = before
        .records
        .into_iter()
        .filter(|record| record.kind == "collaboration-document-v1")
        .collect();
    let retained: Vec<_> = unavailable
        .records
        .into_iter()
        .filter(|record| record.kind == "collaboration-document-v1")
        .collect();
    assert_eq!(retained, old_rows);
    assert_eq!(
        cache.draft(&scope(), "keep").unwrap().as_deref(),
        Some("user intent")
    );
    let raised = DiscussionLimits {
        max_total_bytes: bytes as usize * 3,
        ..DiscussionLimits::default()
    };
    let generation = collaboration::reserve(&mut cache, &current.target).unwrap();
    assert!(collaboration::publish(
        &mut cache,
        &current,
        generation,
        &raised,
        &CancellationToken::default()
    )
    .unwrap());
    let restored = collaboration::collect_cached(
        &cache,
        scope(),
        &[current.target],
        &raised,
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(restored.corpus.documents.len(), 3);
    assert!(!restored.corpus.coverage.complete);
}

#[test]
fn collaboration_index_footprint_guard_precedes_body_deserialization_and_still_preserves_oversized_old_rows(
) {
    let mut cache = Cache::memory().unwrap();
    let old = comment("old phrase", 1);
    publish(&mut cache, &old);
    let id = old.documents().unwrap()[0].key.stable_id();
    let mut poison = cache
        .get(&scope(), "collaboration-document-v1", &id)
        .unwrap()
        .unwrap();
    poison.payload = json!({"oversized invalid Document sentinel":"x".repeat(10000)});
    poison.fetched_at = 2;
    cache.put(&scope(), &poison).unwrap();
    let mut partial = old.clone();
    partial.entries.clear();
    partial.enumeration_complete = false;
    partial.complete = false;
    let limits = DiscussionLimits {
        max_total_bytes: 2048,
        ..DiscussionLimits::default()
    };
    let generation = collaboration::reserve(&mut cache, &partial.target).unwrap();
    assert!(
        collaboration::publish(
            &mut cache,
            &partial,
            generation,
            &limits,
            &CancellationToken::default()
        )
        .unwrap(),
        "Parsing the sentinel as a Document would fail; metadata footprint must stop first"
    );
    let collected = collaboration::collect_cached(
        &cache,
        scope(),
        &[partial.target],
        &limits,
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(collected.corpus.documents.is_empty() && collected.visible_versions.is_empty());
    assert!(collected
        .corpus
        .coverage
        .unavailable
        .iter()
        .any(|reason| reason.contains("no bodies deserialized")));
    assert_eq!(
        cache
            .get(&scope(), "collaboration-document-v1", &id)
            .unwrap()
            .unwrap(),
        poison
    );
}

#[test]
fn collaboration_index_local_only_bare_history_needs_no_worktree_or_github_identity() {
    use std::ffi::OsStr;
    let fixture = git_support::Fixture::linear();
    let bare = fixture.directory.path().join("bare.git");
    git_support::git(
        &fixture.root,
        [
            OsStr::new("clone"),
            OsStr::new("--bare"),
            fixture.root.as_os_str(),
            bare.as_os_str(),
        ],
    );
    let local = Scope::new("local", "workspace").unwrap();
    let selected = commits::collect(
        local.clone(),
        &[CommitSelection {
            repository: "local/messages".into(),
            directory: bare.clone(),
            revision: "HEAD".into(),
            max_commits: 10,
        }],
        &CommitLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(selected.corpus.documents.len(), 2);
    assert!(selected.corpus.coverage.complete);
    assert!(selected
        .corpus
        .documents
        .iter()
        .all(|document| document.key.scope == local && document.metadata.actors.is_empty()));
    assert!(selected
        .corpus
        .documents
        .iter()
        .any(|document| document.body.contains("Root commit")));
    assert_eq!(
        String::from_utf8(git_support::git(&bare, ["rev-parse", "HEAD"]).stdout)
            .unwrap()
            .trim(),
        fixture.head()
    );
}

#[test]
fn collaboration_index_ranked_current_version_fence_hides_old_terms_before_counts_and_reserved_refresh_overrides_future_wallclock(
) {
    use ghtui::search::index::{RankedIndex, RankedLimits};
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("index");
    let mut cache = Cache::memory().unwrap();
    let cancel = CancellationToken::default();
    let old = comment("obsoleteonly phrase", 100);
    publish(&mut cache, &old);
    let selected = collaboration::collect_cached(
        &cache,
        scope(),
        std::slice::from_ref(&old.target),
        &DiscussionLimits::default(),
        &cancel,
    )
    .unwrap();
    let mut index =
        RankedIndex::open(&root, scope(), &cache, RankedLimits::default(), &cancel).unwrap();
    let generation = RankedIndex::reserve_generation(&mut cache, &scope(), "metadata").unwrap();
    index
        .publish_reserved(
            &mut cache,
            "metadata",
            generation,
            &selected.corpus,
            &cancel,
        )
        .unwrap();
    let query = Query::parse("obsoleteonly", SearchMode::Ranked).unwrap();
    assert_eq!(
        index
            .search_source("metadata", &query, &cancel)
            .unwrap()
            .hits
            .len(),
        1,
        "negative control: old lexical derivative really contains the phrase"
    );
    let current = comment("currentonly phrase", 50);
    publish(&mut cache, &current);
    let selected = collaboration::collect_cached(
        &cache,
        scope(),
        &[current.target],
        &DiscussionLimits::default(),
        &cancel,
    )
    .unwrap();
    let hidden = index
        .search_source_current("metadata", &query, &selected.visible_versions, &cancel)
        .unwrap();
    assert!(hidden.hits.is_empty());
    assert_eq!(hidden.coverage.documents, 0);
    let id = selected.corpus.documents[0].key.stable_id();
    let mut future = cache
        .get(&scope(), "search-index-document-v1", &id)
        .unwrap()
        .unwrap();
    future.fetched_at = i64::MAX as u64 / 2;
    cache.put(&scope(), &future).unwrap();
    let generation = RankedIndex::reserve_generation(&mut cache, &scope(), "metadata").unwrap();
    index
        .publish_reserved(
            &mut cache,
            "metadata",
            generation,
            &selected.corpus,
            &cancel,
        )
        .unwrap();
    assert_eq!(
        index
            .search_source_current(
                "metadata",
                &Query::parse("currentonly", SearchMode::Ranked).unwrap(),
                &selected.visible_versions,
                &cancel
            )
            .unwrap()
            .hits
            .len(),
        1
    );
    assert!(index
        .search_source_current("metadata", &query, &selected.visible_versions, &cancel)
        .unwrap()
        .hits
        .is_empty());
    let late = RankedIndex::reserve_generation(&mut cache, &scope(), "metadata").unwrap();
    cache.clear_records(&scope()).unwrap();
    assert!(index
        .publish_reserved(&mut cache, "metadata", late, &selected.corpus, &cancel)
        .is_err());
    assert!(cache
        .list(&scope(), Some("search-index-document-v1"))
        .unwrap()
        .is_empty());
}

#[test]
fn collaboration_index_combined_workspace_exact_regex_ranked_and_saved_metadata_intent_preserve_worktree_and_git_author_provenance(
) {
    use ghtui::search::{
        corpus::{LocalSource, RepositorySelection},
        sources::MetadataSources,
        workspace::SearchWorkspace,
    };
    let fixture = git_support::Fixture::linear();
    fixture.write("metadata-state.txt", b"saved message fixture state\n");
    fixture.commit("Saved title\n\ncommit-only secret phrase");
    let head = fixture.head();
    let index_before = std::fs::read(fixture.root.join(".git/index")).unwrap();
    let mut cache = Cache::memory().unwrap();
    let provider = provider_snapshot(ghtui::providers::gh::DiscussionRecordKind::Review);
    let sources =
        hydration::from_records(scope(), &repository(), &provider, DiscussionKind::Review, 1)
            .unwrap();
    let review = sources
        .iter()
        .find(|source| source.target.category == DiscussionKind::Review)
        .unwrap();
    publish(&mut cache, review);
    let mapping = RepositorySelection {
        repository: "org/app".into(),
        directory: fixture.root.clone(),
        source: LocalSource::Worktree,
        sets: vec!["team".into()],
    };
    let mut workspace = SearchWorkspace::new(scope(), vec![mapping.clone()]);
    workspace
        .set_metadata_sources(MetadataSources {
            discussions: vec![review.target.clone()],
            commits: vec![CommitSelection {
                repository: "org/app".into(),
                directory: fixture.root.clone(),
                revision: "HEAD".into(),
                max_commits: 10,
            }],
        })
        .unwrap();
    let cancel = CancellationToken::default();
    workspace
        .search_cached(
            "only typed record phrase type:comment set:team",
            &cache,
            &cancel,
        )
        .unwrap();
    assert_eq!(workspace.page.as_ref().unwrap().hits.len(), 1);
    workspace
        .validate_metadata_result(0, &cache, &cancel)
        .unwrap();
    workspace.set_mode(SearchMode::Regex);
    workspace
        .search_cached("commit-only .* phrase type:commit", &cache, &cancel)
        .unwrap();
    assert_eq!(workspace.page.as_ref().unwrap().hits.len(), 1);
    let message = &workspace.page.as_ref().unwrap().hits[0].document;
    assert!(message.metadata.actors.is_empty());
    assert_eq!(
        message.metadata.git_author.as_ref().unwrap().name,
        "Fixture Author"
    );
    assert!(
        message.matches(&Query::parse("author:fixture@example.test", SearchMode::Exact).unwrap())
    );
    workspace.save(&cache, "metadata intent", true).unwrap();
    let mut restored = SearchWorkspace::new(scope(), vec![mapping]);
    restored.load_saved(&cache).unwrap();
    restored.restore_saved("metadata intent").unwrap();
    assert_eq!(restored.metadata_sources, workspace.metadata_sources);
    restored.set_mode(SearchMode::Ranked);
    restored
        .search_ranked(
            "commit-only type:commit",
            &mut cache,
            &fixture.directory.path().join("ranked"),
            &cancel,
        )
        .unwrap();
    assert_eq!(restored.page.as_ref().unwrap().hits.len(), 1);
    restored.preview(0).unwrap();
    assert!(restored
        .preview
        .as_ref()
        .unwrap()
        .context
        .contains("no verified GitHub association"));
    assert_eq!(fixture.head(), head);
    assert_eq!(
        std::fs::read(fixture.root.join(".git/index")).unwrap(),
        index_before
    );
}

#[test]
fn collaboration_index_saved_github_scope_never_inherits_inactive_cached_metadata_repositories() {
    use ghtui::search::{sources::MetadataSources, workspace::SearchWorkspace};
    let cache = Cache::memory().unwrap();
    let mut workspace = SearchWorkspace::new(scope(), vec![]);
    let metadata = MetadataSources {
        discussions: vec![comment("private cached phrase", 1).target],
        commits: vec![],
    };
    workspace.set_metadata_sources(metadata.clone()).unwrap();
    workspace.remote_repositories = vec!["org/remote-only".into()];
    workspace.set_mode(SearchMode::GitHub);
    workspace.input = "explicit remote query".into();
    workspace.save(&cache, "remote intent", false).unwrap();
    let saved = workspace.saved.first().unwrap();
    assert_eq!(saved.repositories, ["org/remote-only"]);
    assert!(saved.metadata_sources.is_empty());
    assert!(saved.sources.is_empty());

    let mut restored = SearchWorkspace::new(scope(), vec![]);
    restored.set_metadata_sources(metadata).unwrap();
    restored.load_saved(&cache).unwrap();
    restored.restore_saved("remote intent").unwrap();
    assert_eq!(restored.mode, SearchMode::GitHub);
    assert_eq!(restored.remote_repositories, ["org/remote-only"]);
    assert!(restored.metadata_sources.is_empty());
    assert_eq!(restored.input, "explicit remote query");
    assert!(restored.page.is_none());
}

#[test]
fn collaboration_index_streamed_projection_matches_exact_full_canonical_serialization() {
    use sha2::{Digest, Sha256};
    let mut document = comment("seed", 1).documents().unwrap().remove(0);
    document.body = "large \"quoted\" 日本語 body\\\n".repeat(80_000);
    let expected = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&document).unwrap())
    );
    assert_eq!(document.projection_checksum().unwrap(), expected);
    let mut edited = document.clone();
    edited.body.push_str("changed");
    assert_ne!(edited.projection_checksum().unwrap(), expected);
    edited = document;
    edited.metadata.discussion.as_mut().unwrap().outdated = true;
    assert_ne!(edited.projection_checksum().unwrap(), expected);
}

#[test]
fn collaboration_index_known_superseded_node_does_not_resurrect_on_later_unobserved_partial_refresh(
) {
    let mut cache = Cache::memory().unwrap();
    let old = comment("obsolete small body", 1);
    publish(&mut cache, &old);
    let mut oversized = comment(&"new oversized edited body ".repeat(200), 2);
    oversized.enumeration_complete = false;
    oversized.complete = false;
    let limits = DiscussionLimits {
        max_document_bytes: 2048,
        ..DiscussionLimits::default()
    };
    let generation = collaboration::reserve(&mut cache, &oversized.target).unwrap();
    assert!(collaboration::publish(
        &mut cache,
        &oversized,
        generation,
        &limits,
        &CancellationToken::default()
    )
    .unwrap());
    let hidden = collaboration::collect_cached(
        &cache,
        scope(),
        std::slice::from_ref(&old.target),
        &DiscussionLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(hidden.corpus.documents.is_empty());

    let mut unknown = oversized;
    unknown.entries.clear();
    unknown.fetched_at = 3;
    publish(&mut cache, &unknown);
    let still_hidden = collaboration::collect_cached(
        &cache,
        scope(),
        std::slice::from_ref(&old.target),
        &DiscussionLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(still_hidden.corpus.documents.is_empty());
    assert!(still_hidden.visible_versions.is_empty());
    assert!(cache
        .sync_frame(&scope(), &old.target.source())
        .unwrap()
        .unwrap()
        .records
        .iter()
        .any(|record| record.kind == "collaboration-document-v1"));

    let mut current = comment("observed current recovery", 4);
    current.enumeration_complete = false;
    current.complete = false;
    publish(&mut cache, &current);
    let recovered = collaboration::collect_cached(
        &cache,
        scope(),
        &[old.target],
        &DiscussionLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(recovered.corpus.documents.len(), 1);
    assert_eq!(
        recovered.corpus.documents[0].body,
        "observed current recovery"
    );
}

#[test]
fn collaboration_index_known_excluded_thread_allows_only_freshly_observed_members_on_partial_recovery(
) {
    let mut cache = Cache::memory().unwrap();
    let old = inline(
        &[thread(
            "PRRT_excluded",
            &[("PRRC_one", "obsolete one"), ("PRRC_two", "obsolete two")],
        )],
        true,
        &["PRRT_excluded"],
        1,
    );
    publish(&mut cache, &old);
    let mut excluded = inline(&[], false, &[], 2);
    excluded.excluded_threads.insert("PRRT_excluded".into());
    publish(&mut cache, &excluded);
    assert!(collaboration::collect_cached(
        &cache,
        scope(),
        std::slice::from_ref(&old.target),
        &DiscussionLimits::default(),
        &CancellationToken::default()
    )
    .unwrap()
    .corpus
    .documents
    .is_empty());
    let current = inline(
        &[thread("PRRT_excluded", &[("PRRC_one", "current one")])],
        false,
        &[],
        3,
    );
    publish(&mut cache, &current);
    let selected = collaboration::collect_cached(
        &cache,
        scope(),
        &[old.target],
        &DiscussionLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(selected.corpus.documents.len(), 1);
    assert_eq!(selected.corpus.documents[0].body, "current one");
    assert!(!selected.corpus.coverage.complete);
}

#[test]
fn collaboration_index_known_obsolete_visibility_survives_unavailable_budget_then_explicit_cap_recovery(
) {
    let mut cache = Cache::memory().unwrap();
    let old = comment("obsolete retained body", 1);
    publish(&mut cache, &old);
    let mut excluded = old.clone();
    excluded.entries.clear();
    excluded.excluded_nodes.insert("IC_stable".into());
    excluded.enumeration_complete = false;
    excluded.complete = false;
    excluded.fetched_at = 2;
    publish(&mut cache, &excluded);

    let (_, bytes) = cache
        .sync_footprint(&scope(), &old.target.source())
        .unwrap();
    let mut current = comment(&"fresh ".repeat(128), 3);
    current.enumeration_complete = false;
    current.complete = false;
    current.entries[0].node_id = "IC_other".into();
    current.entries[0].numeric_id = Some(14);
    current.entries[0].context.resource_node = "IC_other".into();
    current.entries[0].context.resource_numeric_id = Some(14);
    let limits = DiscussionLimits {
        max_total_bytes: bytes as usize + 64,
        ..DiscussionLimits::default()
    };
    let generation = collaboration::reserve(&mut cache, &current.target).unwrap();
    assert!(collaboration::publish(
        &mut cache,
        &current,
        generation,
        &limits,
        &CancellationToken::default()
    )
    .unwrap());
    let unavailable = collaboration::collect_cached(
        &cache,
        scope(),
        std::slice::from_ref(&old.target),
        &DiscussionLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(unavailable.corpus.documents.is_empty());
    assert!(!unavailable.corpus.coverage.unavailable.is_empty());

    publish(&mut cache, &current);
    let recovered = collaboration::collect_cached(
        &cache,
        scope(),
        &[old.target],
        &DiscussionLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(recovered.corpus.documents.len(), 1);
    assert_eq!(recovered.corpus.documents[0].body, current.entries[0].body);
    assert!(cache
        .sync_frame(&scope(), &current.target.source())
        .unwrap()
        .unwrap()
        .records
        .iter()
        .any(|record| record.kind == "collaboration-document-v1"
            && record.payload["body"] == "obsolete retained body"));
}
