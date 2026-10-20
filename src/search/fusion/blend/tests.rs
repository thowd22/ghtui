//! Staged provider-free contracts: real scoped cache generations, not transport stubs.
#[path = "discovered_tests.rs"]
mod discovered_tests;
#[path = "relevance_quality_tests.rs"]
mod relevance_quality_tests;

use super::*;
use crate::{
    providers::gh::Actor,
    search::{
        collaboration::{Access, DiscussionEntry, DiscussionSnapshot, ParentKind},
        model::{DiscussionContext, DocumentKey, Metadata},
    },
};

fn scope() -> Scope {
    Scope::github("github.invalid", Some(42)).unwrap()
}
fn context() -> BlendContext {
    let query = Query::parse("needle", SearchMode::Ranked).unwrap();
    BlendContext {
        scope: scope(),
        local_query: query.clone(),
        query,
        local_repositories: BTreeSet::from(["org/app".into()]),
        remote_repositories: BTreeSet::from(["org/app".into()]),
        repository_sets: BTreeMap::new(),
        policy: BlendPolicy {
            enabled: true,
            sources: BTreeSet::from([RankingSource::GithubPullRequests]),
            ..Default::default()
        },
    }
}
fn snapshot(category: DiscussionKind, body: &str) -> DiscussionSnapshot {
    let target = DiscussionTarget {
        scope: scope(),
        repository_id: 101,
        repository: "org/app".into(),
        parent_kind: ParentKind::PullRequest,
        parent_node: "PR_parent".into(),
        number: 7,
        category,
    };
    let node = if category == DiscussionKind::PullRequestBody {
        "PR_parent"
    } else {
        "IC_distinct"
    };
    DiscussionSnapshot {
        target,
        entries: vec![DiscussionEntry {
            node_id: node.into(),
            numeric_id: Some(700),
            title: "A parent and a distinct comment".into(),
            body: body.into(),
            author: Some(Actor {
                login: "alice".into(),
                actor_type: "User".into(),
                ..Default::default()
            }),
            created_at: None,
            updated_at: None,
            context: DiscussionContext {
                kind: category,
                repository_id: 101,
                parent_node: "PR_parent".into(),
                parent_number: 7,
                resource_node: node.into(),
                resource_numeric_id: Some(700),
                thread_id: None,
                path: None,
                commit_sha: None,
                side: None,
                line: None,
                start_line: None,
                outdated: false,
                resolved: None,
                url: None,
            },
        }],
        access: Access::Allowed,
        enumeration_complete: true,
        complete: true,
        reasons: vec![],
        fetched_at: 10,
        head_sha: Some("a".repeat(40)),
        parent_state: Some("open".into()),
        parent_labels: Some(vec![]),
        excluded_nodes: BTreeSet::new(),
        excluded_threads: BTreeSet::new(),
        thread_enumeration: None,
    }
}
fn publish(cache: &mut Cache, snapshot: &DiscussionSnapshot) {
    let generation = collaboration::reserve(cache, &snapshot.target).unwrap();
    assert!(collaboration::publish(
        cache,
        snapshot,
        generation,
        &DiscussionLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap());
}
fn current(
    cache: &Cache,
    targets: &[DiscussionTarget],
    context: &BlendContext,
) -> CurrentVisibility {
    CurrentVisibility::collect(
        cache,
        context,
        targets,
        &DiscussionLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap()
}
fn cached(cache: &Cache, targets: &[DiscussionTarget]) -> Vec<Document> {
    collaboration::collect_cached(
        cache,
        scope(),
        targets,
        &DiscussionLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap()
    .corpus
    .documents
}
fn hit(document: Document, score: f32) -> SearchHit {
    let local_file = matches!(document.destination, Destination::LocalFile { .. });
    SearchHit {
        snippet: document.body.clone(),
        document: Arc::new(document),
        line: usize::from(local_file),
        column: usize::from(local_file),
        highlights: vec![],
        score,
        matched_fields: vec!["initial engine evidence".into()],
    }
}
fn local_page(context: &BlendContext, hits: Vec<SearchHit>) -> SearchPage {
    SearchPage {
        query: context.local_query.clone(),
        coverage: Coverage {
            complete: true,
            documents: hits.len(),
            repositories: vec!["org/app".into()],
            ..Default::default()
        },
        hits,
    }
}
fn local_file(name: &str) -> Document {
    let path = RawPath::new(name.as_bytes().to_vec());
    Document {
        key: DocumentKey {
            scope: scope(),
            repository: "org/app".into(),
            revision: "working".into(),
            kind: ResultKind::File,
            path: Some(path.clone()),
            resource: name.into(),
        },
        source: SourceKind::Worktree,
        title: name.into(),
        body: "needle local working content".into(),
        metadata: Metadata {
            complete: true,
            ..Default::default()
        },
        destination: Destination::LocalFile {
            directory: "/tmp/blend-contract-repo".into(),
            path,
            revision: None,
            line: 1,
            column: 1,
        },
    }
}
fn remote_parent(id: &str, identity: RemoteIdentity) -> RemoteCandidate {
    RemoteCandidate {
        hit: hit(
            Document {
                key: DocumentKey {
                    scope: scope(),
                    repository: "org/app".into(),
                    revision: "remote-current-item".into(),
                    kind: ResultKind::PullRequest,
                    path: None,
                    resource: format!("github-search:{id}"),
                },
                source: SourceKind::GitHub,
                title: "Remote indexed parent".into(),
                body: "needle stale remote body".into(),
                metadata: Metadata::default(),
                destination: Destination::PullRequest {
                    host: "github.invalid".into(),
                    repository: "org/app".into(),
                    number: 7,
                    comment_id: None,
                    thread_id: None,
                },
            },
            900_000.0,
        ),
        identity,
    }
}
fn node_identity(node: &str) -> RemoteIdentity {
    RemoteIdentity::OpaqueNode {
        node_id: node.into(),
        database_id: (node == "PR_parent").then_some(700),
        repository_id: Some(101),
    }
}
fn remote_number(mut candidate: RemoteCandidate, number: u64) -> RemoteCandidate {
    if let Destination::PullRequest { number: actual, .. } =
        &mut Arc::make_mut(&mut candidate.hit.document).destination
    {
        *actual = number;
    }
    candidate
}
fn remote_list(context: &BlendContext, candidates: Vec<RemoteCandidate>) -> RemoteRankList {
    RemoteRankList {
        source: RankingSource::GithubPullRequests,
        scope: scope(),
        query: context
            .remote_query(RankingSource::GithubPullRequests)
            .unwrap(),
        coverage: Coverage {
            complete: true,
            documents: candidates.len(),
            repositories: vec!["org/app".into()],
            ..Default::default()
        },
        candidates,
        status: SourceStatus::Complete,
    }
}
fn run(
    cache: &Cache,
    context: &BlendContext,
    local: &SearchPage,
    remote: &[RemoteRankList],
    visibility: &CurrentVisibility,
) -> BlendPage {
    blend(
        cache,
        context,
        local,
        remote,
        visibility,
        &CancellationToken::default(),
    )
    .unwrap()
}

#[test]
fn collaboration_index_blend_raw_scores_cannot_outvote_canonical_rank_agreement_or_merge_comments()
{
    let context = context();
    let mut cache = Cache::memory().unwrap();
    let body = snapshot(
        DiscussionKind::PullRequestBody,
        "needle current canonical parent",
    );
    let comment = snapshot(
        DiscussionKind::ConversationComment,
        "needle distinct comment",
    );
    publish(&mut cache, &body);
    publish(&mut cache, &comment);
    let targets = [body.target, comment.target];
    let documents = cached(&cache, &targets);
    let parent = documents
        .iter()
        .find(|document| document.key.kind == ResultKind::PullRequest)
        .unwrap();
    let comment = documents
        .iter()
        .find(|document| document.key.kind == ResultKind::Comment)
        .unwrap();
    let local = local_page(
        &context,
        vec![
            hit(local_file("file.txt"), 99_000.0),
            hit(parent.clone(), 0.00001),
            hit(comment.clone(), 50_000.0),
        ],
    );
    let remote = remote_list(
        &context,
        vec![
            remote_number(remote_parent("other", node_identity("PR_other")), 8),
            remote_parent("PR_parent", node_identity("PR_parent")),
        ],
    );
    let output = run(
        &cache,
        &context,
        &local,
        &[remote],
        &current(&cache, &targets, &context),
    );
    assert_eq!(output.page.hits.len(), 4);
    assert_eq!(output.page.hits[0].document.key, parent.key);
    assert_eq!(
        output.page.hits[0].snippet,
        "needle current canonical parent"
    );
    assert_eq!(
        output.contributions[&parent.key.stable_id()],
        [
            (RankingSource::LocalLexical, 2),
            (RankingSource::GithubPullRequests, 2)
        ]
    );
    assert!(output
        .page
        .hits
        .iter()
        .any(|hit| hit.document.key == comment.key));
    assert_eq!(
        output.origins[&parent.key.stable_id()],
        CandidateOrigin::Local(1)
    );
}

#[test]
fn collaboration_index_blend_remote_parent_outside_local_topk_still_shows_authorized_current_body()
{
    let mut context = context();
    context
        .repository_sets
        .insert("org/app".into(), vec!["pinned".into()]);
    let mut cache = Cache::memory().unwrap();
    let body = snapshot(
        DiscussionKind::PullRequestBody,
        "Current body edited after GitHub indexing — 🦀",
    );
    publish(&mut cache, &body);
    let targets = [body.target];
    let visibility = current(&cache, &targets, &context);
    let mut candidate = remote_parent("PR_parent", node_identity("PR_parent"));
    candidate.hit.highlights.push(0..6);
    let output = run(
        &cache,
        &context,
        &local_page(&context, vec![]),
        &[remote_list(&context, vec![candidate])],
        &visibility,
    );
    let hit = &output.page.hits[0];
    assert_eq!(hit.document.source, SourceKind::Collaboration);
    assert_eq!(hit.document.body, body.entries[0].body);
    assert!(!hit.snippet.contains("stale remote"));
    assert!(hit.highlights.is_empty());
    assert_eq!(hit.document.metadata.repository_sets, ["pinned"]);
    assert!(matches!(
        output.origins[&hit.document.key.stable_id()],
        CandidateOrigin::CurrentCanonical(_)
    ));
    assert!(hit.matched_fields[0].contains("indexed parent match"));
}

#[test]
fn collaboration_index_blend_numeric_opaque_and_unknown_legacy_ids_never_alias_by_display_id() {
    let context = context();
    let cache = Cache::memory().unwrap();
    let visibility = current(&cache, &[], &context);
    let numeric = RemoteIdentity::Numeric {
        database_id: 700,
        repository_id: Some(101),
    };
    let candidates = vec![
        remote_parent("700", node_identity("700")),
        remote_parent("700", numeric),
        remote_parent("700", RemoteIdentity::Unavailable),
        remote_parent("700", RemoteIdentity::Unavailable),
    ];
    let output = run(
        &cache,
        &context,
        &local_page(&context, vec![]),
        &[remote_list(&context, candidates)],
        &visibility,
    );
    let keys = output
        .page
        .hits
        .iter()
        .map(|hit| hit.document.key.stable_id())
        .collect::<BTreeSet<_>>();
    assert_eq!(output.page.hits.len(), 4);
    assert_eq!(keys.len(), 4);
    assert_eq!(output.origins.len(), 4);
}

#[test]
fn collaboration_index_blend_fenced_parent_cannot_reappear_via_node_numeric_or_missing_remote_identity(
) {
    let context = context();
    let mut cache = Cache::memory().unwrap();
    let body = snapshot(DiscussionKind::PullRequestBody, "needle obsolete body");
    publish(&mut cache, &body);
    let targets = [body.target.clone()];
    let old = cached(&cache, &targets).remove(0);
    collaboration::invalidate(&mut cache, &body.target).unwrap();
    let candidates = vec![
        remote_parent("PR_parent", node_identity("PR_parent")),
        remote_parent(
            "700",
            RemoteIdentity::Numeric {
                database_id: 700,
                repository_id: Some(101),
            },
        ),
        remote_parent("legacy", RemoteIdentity::Unavailable),
        remote_number(
            remote_parent("fresh-unselected", node_identity("PR_fresh")),
            8,
        ),
    ];
    let output = run(
        &cache,
        &context,
        &local_page(&context, vec![hit(old, 10.0)]),
        &[remote_list(&context, candidates)],
        &current(&cache, &targets, &context),
    );
    assert_eq!(output.page.hits.len(), 1);
    assert!(output.page.hits[0]
        .document
        .title
        .contains("Remote indexed"));
    assert_eq!(output.reports[0].suppressed, 3);
    assert!(!output.page.coverage.complete);
    assert!(cache
        .sync_checkpoint(&scope(), &body.target.source())
        .unwrap()
        .is_some());
}

#[test]
fn collaboration_index_blend_unavailable_capped_source_is_not_authorized_by_a_published_epoch() {
    let context = context();
    let mut cache = Cache::memory().unwrap();
    let mut body = snapshot(DiscussionKind::PullRequestBody, "needle initially allowed");
    publish(&mut cache, &body);
    body.access = Access::Unavailable("transport unavailable, not permission revocation".into());
    body.entries.clear();
    body.complete = false;
    publish(&mut cache, &body);
    let targets = [body.target.clone()];
    assert!(bound_generation(&cache, &body.target).unwrap().is_some());
    let output = run(
        &cache,
        &context,
        &local_page(&context, vec![]),
        &[remote_list(
            &context,
            vec![remote_parent("PR_parent", node_identity("PR_parent"))],
        )],
        &current(&cache, &targets, &context),
    );
    assert!(output.page.hits.is_empty());
    assert_eq!(output.reports[0].suppressed, 1);
}

#[test]
fn collaboration_index_blend_epoch_or_principal_change_fails_closed_before_old_counts() {
    let context = context();
    let mut cache = Cache::memory().unwrap();
    let body = snapshot(DiscussionKind::PullRequestBody, "needle current");
    publish(&mut cache, &body);
    let visibility = current(&cache, std::slice::from_ref(&body.target), &context);
    let local = local_page(&context, vec![]);
    collaboration::invalidate(&mut cache, &body.target).unwrap();
    assert!(blend(
        &cache,
        &context,
        &local,
        &[],
        &visibility,
        &CancellationToken::default()
    )
    .unwrap_err()
    .0
    .contains("visibility changed"));
    let visibility = current(&cache, &[], &context);
    let mut remote = remote_list(&context, vec![]);
    remote.scope = Scope::github("github.invalid", Some(43)).unwrap();
    assert!(blend(
        &cache,
        &context,
        &local,
        &[remote],
        &visibility,
        &CancellationToken::default()
    )
    .unwrap_err()
    .0
    .contains("Account identity changed"));
}

#[test]
fn collaboration_index_blend_exact_scope_and_query_constraints_are_not_silently_broadened() {
    let mut context = context();
    context.query = Query::parse("needle rev:HEAD type:file", SearchMode::Ranked).unwrap();
    context.local_query = context.query.clone();
    context
        .local_query
        .filters
        .insert("rev".into(), vec!["a".repeat(40)]);
    assert!(context.validate().is_ok());
    assert!(context.remote_query(RankingSource::GithubCode).is_err());
    context.local_query.filters.remove("type");
    assert!(context.validate().is_err());
    context = super::tests::context();
    context.query.mode = SearchMode::Exact;
    context.local_query.mode = SearchMode::Exact;
    assert!(context.validate().is_err());
    context = super::tests::context();
    context.remote_repositories.clear();
    assert!(context.validate().is_err());
    let cache = Cache::memory().unwrap();
    context = super::tests::context();
    let visibility = current(&cache, &[], &context);
    let mut remote = remote_list(
        &context,
        vec![remote_parent("other-repo", node_identity("PR_other"))],
    );
    Arc::make_mut(&mut remote.candidates[0].hit.document)
        .key
        .repository = "unselected/repo".into();
    if let Destination::PullRequest { repository, .. } =
        &mut Arc::make_mut(&mut remote.candidates[0].hit.document).destination
    {
        *repository = "unselected/repo".into();
    }
    assert!(blend(
        &cache,
        &context,
        &local_page(&context, vec![]),
        &[remote],
        &visibility,
        &CancellationToken::default()
    )
    .unwrap_err()
    .0
    .contains("declared remote scope"));
}

#[test]
fn collaboration_index_blend_partial_or_failed_provider_retains_local_with_real_caps_and_status() {
    let mut context = context();
    context.policy.per_source_results = 1;
    context.policy.max_results = 1;
    let cache = Cache::memory().unwrap();
    let visibility = current(&cache, &[], &context);
    let local = local_page(&context, vec![hit(local_file("local.txt"), 2.0)]);
    let mut unavailable = remote_list(&context, vec![]);
    unavailable.status = SourceStatus::Unavailable("rate limited; retry later".into());
    let output = run(&cache, &context, &local, &[unavailable], &visibility);
    assert_eq!(output.page.hits[0].document.key, local.hits[0].document.key);
    assert!(!output.page.coverage.complete);
    assert!(output.page.coverage.unavailable[0].contains("rate limited"));
    let remote = remote_list(
        &context,
        vec![
            remote_parent("one", node_identity("PR_one")),
            remote_parent("two", node_identity("PR_two")),
        ],
    );
    let output = run(&cache, &context, &local, &[remote], &visibility);
    assert_eq!(output.page.hits.len(), 1);
    assert_eq!(output.reports[0].loaded, 2);
    assert_eq!(output.reports[0].admitted, 1);
    assert!(matches!(output.reports[0].status, SourceStatus::Partial(_)));
    assert!(output
        .page
        .coverage
        .reasons
        .iter()
        .any(|reason| reason.contains("Final blended result cap")));
}

#[test]
fn collaboration_index_blend_off_preserves_local_score_and_never_accepts_remote_results() {
    let mut context = context();
    let remote = remote_list(&context, vec![]);
    context.policy.enabled = false;
    context.remote_repositories.clear();
    let cache = Cache::memory().unwrap();
    let visibility = current(&cache, &[], &context);
    let local = local_page(&context, vec![hit(local_file("local.txt"), 700.0)]);
    let output = run(&cache, &context, &local, &[], &visibility);
    assert_eq!(output.page.hits, local.hits);
    assert_eq!(output.page.coverage, local.coverage);
    assert!(output.contributions.is_empty() && output.reports.is_empty());
    assert!(blend(
        &cache,
        &context,
        &local,
        &[remote],
        &visibility,
        &CancellationToken::default()
    )
    .unwrap_err()
    .0
    .contains("explicit opt-in"));
}

#[test]
fn collaboration_index_blend_canonical_repository_contradiction_cannot_fall_back_to_stale_local() {
    let context = context();
    let mut cache = Cache::memory().unwrap();
    let body = snapshot(DiscussionKind::PullRequestBody, "needle current canonical");
    publish(&mut cache, &body);
    let targets = [body.target];
    let mut wrong = node_identity("PR_parent");
    if let RemoteIdentity::OpaqueNode { repository_id, .. } = &mut wrong {
        *repository_id = Some(202);
    }
    let local = local_page(
        &context,
        cached(&cache, &targets)
            .into_iter()
            .map(|document| hit(document, 1.0))
            .collect(),
    );
    let remote = remote_list(&context, vec![remote_parent("PR_parent", wrong)]);
    assert!(blend(
        &cache,
        &context,
        &local,
        &[remote],
        &current(&cache, &targets, &context),
        &CancellationToken::default()
    )
    .unwrap_err()
    .0
    .contains("Canonical repository identity changed"));
}

#[test]
fn collaboration_index_blend_missing_sources_and_cancelled_workers_do_not_claim_successful_empty() {
    let context = context();
    let cache = Cache::memory().unwrap();
    let visibility = current(&cache, &[], &context);
    let local = local_page(&context, vec![]);
    let output = run(&cache, &context, &local, &[], &visibility);
    assert!(!output.page.coverage.complete);
    assert!(matches!(
        output.reports[0].status,
        SourceStatus::Unavailable(_)
    ));
    let cancel = CancellationToken::default();
    cancel.cancel();
    assert!(blend(&cache, &context, &local, &[], &visibility, &cancel)
        .unwrap_err()
        .0
        .contains("cancelled"));
}

#[test]
fn collaboration_index_blend_omitted_or_deleted_selected_comment_never_reappears_as_parent_fragment(
) {
    let context = context();
    let mut cache = Cache::memory().unwrap();
    let mut comment = snapshot(
        DiscussionKind::ConversationComment,
        "needle subsequently deleted comment",
    );
    publish(&mut cache, &comment);
    let targets = [comment.target.clone()];
    let old = cached(&cache, &targets).remove(0);
    comment.entries.clear();
    publish(&mut cache, &comment);
    let remote = remote_list(
        &context,
        vec![remote_parent("PR_parent", node_identity("PR_parent"))],
    );
    let output = run(
        &cache,
        &context,
        &local_page(&context, vec![hit(old, 1.0)]),
        &[remote],
        &current(&cache, &targets, &context),
    );
    assert!(output.page.hits.is_empty());
    assert_eq!(output.reports[0].suppressed, 1);
    assert!(cached(&cache, &targets).is_empty());
}

#[test]
fn collaboration_index_blend_collection_cap_and_destination_contradiction_cannot_bypass_selection()
{
    let context = context();
    let mut cache = Cache::memory().unwrap();
    let body = snapshot(
        DiscussionKind::PullRequestBody,
        "needle current canonical body",
    );
    publish(&mut cache, &body);
    let targets = [body.target];
    let capped = CurrentVisibility::collect(
        &cache,
        &context,
        &targets,
        &DiscussionLimits {
            max_total_bytes: 1,
            ..Default::default()
        },
        &CancellationToken::default(),
    )
    .unwrap();
    let output = run(
        &cache,
        &context,
        &local_page(&context, vec![]),
        &[remote_list(
            &context,
            vec![remote_parent("PR_parent", node_identity("PR_parent"))],
        )],
        &capped,
    );
    assert!(output.page.hits.is_empty());
    assert_eq!(output.reports[0].suppressed, 1);
    let mut candidate = remote_parent("PR_parent", node_identity("PR_parent"));
    if let Destination::PullRequest { number, .. } =
        &mut Arc::make_mut(&mut candidate.hit.document).destination
    {
        *number = 8;
    }
    let remote = remote_list(&context, vec![candidate]);
    assert!(blend(
        &cache,
        &context,
        &local_page(&context, vec![]),
        &[remote],
        &current(&cache, &targets, &context),
        &CancellationToken::default()
    )
    .unwrap_err()
    .0
    .contains("parent destination changed"));
}

#[test]
fn collaboration_index_blend_changed_actual_parent_node_or_database_id_cannot_return_stale_parent()
{
    let context = context();
    let mut cache = Cache::memory().unwrap();
    let body = snapshot(
        DiscussionKind::PullRequestBody,
        "needle current before replacement",
    );
    publish(&mut cache, &body);
    let targets = [body.target];
    let visibility = current(&cache, &targets, &context);
    let local = local_page(
        &context,
        cached(&cache, &targets)
            .into_iter()
            .map(|document| hit(document, 1.0))
            .collect(),
    );
    let replacement = remote_list(
        &context,
        vec![remote_parent("PR_replaced", node_identity("PR_replaced"))],
    );
    assert!(blend(
        &cache,
        &context,
        &local,
        &[replacement],
        &visibility,
        &CancellationToken::default()
    )
    .unwrap_err()
    .0
    .contains("Canonical parent identity changed"));
    let contradictory = RemoteIdentity::OpaqueNode {
        node_id: "PR_parent".into(),
        database_id: Some(900),
        repository_id: Some(101),
    };
    let changed_database = remote_list(&context, vec![remote_parent("PR_parent", contradictory)]);
    assert!(blend(
        &cache,
        &context,
        &local,
        &[changed_database],
        &visibility,
        &CancellationToken::default()
    )
    .unwrap_err()
    .0
    .contains("database identity changed"));
}

#[test]
fn collaboration_index_blend_indexed_blob_raw_paths_never_alias_mutable_or_saved_files() {
    let mut context = context();
    context.policy.sources = BTreeSet::from([RankingSource::GithubCode]);
    let cache = Cache::memory().unwrap();
    let visibility = current(&cache, &[], &context);
    let working = local_file("Case.txt");
    let mut saved = working.clone();
    saved.source = SourceKind::SavedCommit;
    saved.key.revision = "a".repeat(40);
    if let Destination::LocalFile { revision, .. } = &mut saved.destination {
        *revision = Some(saved.key.revision.clone());
    }
    let local = local_page(&context, vec![hit(working, 1000.0), hit(saved, 0.1)]);
    let path = RawPath::new(b"Case.txt".to_vec());
    let blob = "b".repeat(40);
    let mut code = local_file("Case.txt");
    code.source = SourceKind::GitHub;
    code.key.revision = format!("indexed-blob:{blob}");
    code.destination = Destination::RemoteCodeCandidate {
        host: context.scope.host.clone(),
        repository: "org/app".into(),
        path: path.clone(),
        blob_sha: blob.clone(),
    };
    let candidate = RemoteCandidate {
        hit: hit(code, 1_000_000.0),
        identity: RemoteIdentity::CodeBlob {
            path,
            blob_sha: blob,
            repository_id: Some(101),
        },
    };
    let mut remote = RemoteRankList {
        source: RankingSource::GithubCode,
        scope: scope(),
        query: context.remote_query(RankingSource::GithubCode).unwrap(),
        candidates: vec![candidate],
        coverage: Coverage {
            complete: true,
            ..Default::default()
        },
        status: SourceStatus::Complete,
    };
    let output = run(
        &cache,
        &context,
        &local,
        std::slice::from_ref(&remote),
        &visibility,
    );
    assert_eq!(output.page.hits.len(), 3);
    assert!(output.page.hits.iter().any(|hit| matches!(
        hit.document.destination,
        Destination::RemoteCodeCandidate { .. }
    )));
    assert_eq!(output.origins.len(), 3);
    if let RemoteIdentity::CodeBlob { path, .. } = &mut remote.candidates[0].identity {
        *path = RawPath::new(b"case.txt".to_vec());
    }
    assert!(blend(
        &cache,
        &context,
        &local,
        &[remote],
        &visibility,
        &CancellationToken::default()
    )
    .unwrap_err()
    .0
    .contains("raw path/blob"));
}

#[test]
fn collaboration_index_blend_oversized_invalid_or_duplicate_source_projections_are_rejected() {
    let context = context();
    let cache = Cache::memory().unwrap();
    let visibility = current(&cache, &[], &context);
    let local = local_page(&context, vec![]);
    let remote = remote_list(
        &context,
        vec![remote_parent("PR_parent", node_identity("PR_parent"))],
    );
    assert!(blend(
        &cache,
        &context,
        &local,
        &[remote.clone(), remote.clone()],
        &visibility,
        &CancellationToken::default()
    )
    .unwrap_err()
    .0
    .contains("Duplicate"));
    let mut oversized = remote.clone();
    oversized.candidates = vec![remote.candidates[0].clone(); 5001];
    assert!(blend(
        &cache,
        &context,
        &local,
        &[oversized],
        &visibility,
        &CancellationToken::default()
    )
    .unwrap_err()
    .0
    .contains("absolute 5000"));
    let mut malformed = remote;
    malformed.candidates[0].hit.snippet = "🦀needle".into();
    malformed.candidates[0].hit.highlights = std::iter::once(1..4).collect();
    assert!(blend(
        &cache,
        &context,
        &local,
        &[malformed],
        &visibility,
        &CancellationToken::default()
    )
    .unwrap_err()
    .0
    .contains("UTF-8 highlights"));
}
