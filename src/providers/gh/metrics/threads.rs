//! Undeclared current thread metrics sidecar. Preserve legacy ReviewThread protocols.
//! Root must freshly guard principal/access/original PR journal before commit/read/UI.
use super::*;
mod budget;
mod model;
mod parse;
use budget::*;
pub use model::*;
use parse::*;

// No originalPath/originalDiffSide exists; comment.path and thread sides are candidates.
// The explicit replyTo null, not connection position, establishes the original comment.
pub const METRICS_REVIEW_THREADS_QUERY: &str = r#"query($id:ID!,$first:Int!,$after:String,$commentsFirst:Int!,$includeComments:Boolean!){node(id:$id){__typename ... on PullRequest{id fullDatabaseId number headRefOid repository{id databaseId nameWithOwner} reviewThreads(first:$first,after:$after){totalCount pageInfo{hasNextPage endCursor} nodes{...MetricThread}}}}}
fragment MetricParent on PullRequest{id fullDatabaseId number headRefOid repository{id databaseId nameWithOwner}}
fragment MetricComment on PullRequestReviewComment{id fullDatabaseId body createdAt updatedAt path line startLine originalLine originalStartLine originalCommit{oid} commit{oid} replyTo{id} author{__typename login avatarUrl ... on User{databaseId name}} pullRequest{...MetricParent} repository{id databaseId nameWithOwner}}
fragment MetricThread on PullRequestReviewThread{id subjectType isResolved isOutdated path diffSide startDiffSide line startLine originalLine originalStartLine viewerCanReply viewerCanResolve viewerCanUnresolve resolvedBy{__typename databaseId login name avatarUrl} pullRequest{...MetricParent} repository{id databaseId nameWithOwner} comments(first:$commentsFirst) @include(if:$includeComments){totalCount pageInfo{hasNextPage endCursor} nodes{...MetricComment}}}"#;
pub const METRICS_THREAD_COMMENTS_QUERY: &str = r#"query($id:ID!,$first:Int!,$after:String){node(id:$id){__typename ... on PullRequestReviewThread{id subjectType isResolved isOutdated path diffSide startDiffSide line startLine originalLine originalStartLine viewerCanReply viewerCanResolve viewerCanUnresolve resolvedBy{__typename databaseId login name avatarUrl} pullRequest{...MetricParent} repository{id databaseId nameWithOwner} comments(first:$first,after:$after){totalCount pageInfo{hasNextPage endCursor} nodes{...MetricComment}}}}}
fragment MetricParent on PullRequest{id fullDatabaseId number headRefOid repository{id databaseId nameWithOwner}}
fragment MetricComment on PullRequestReviewComment{id fullDatabaseId body createdAt updatedAt path line startLine originalLine originalStartLine originalCommit{oid} commit{oid} replyTo{id} author{__typename login avatarUrl ... on User{databaseId name}} pullRequest{...MetricParent} repository{id databaseId nameWithOwner}}"#;

fn node_value<'a>(value: &'a Value, typename: &str) -> GithubResult<&'a Value> {
    let node = &value["data"]["node"];
    if node["__typename"].as_str() != Some(typename) {
        return Err(error(
            GithubErrorKind::IdentityChanged,
            "GraphQL metric node is absent or a different canonical resource kind",
        ));
    }
    Ok(node)
}
fn private_failure(err: &GithubError) -> bool {
    matches!(
        err.kind,
        GithubErrorKind::Authentication
            | GithubErrorKind::IdentityChanged
            | GithubErrorKind::Permission
            | GithubErrorKind::NotFound
            | GithubErrorKind::StaleHead
    )
}
fn doc_size(value: &Value) -> GithubResult<usize> {
    serde_json::to_vec(value)
        .map(|value| value.len())
        .map_err(|err| error(GithubErrorKind::Json, err.to_string()))
}
#[derive(Default)]
struct SeenComments {
    nodes: BTreeSet<String>,
    numeric: BTreeMap<u64, String>,
}
fn comments_page(
    target: &mut MetricsReviewThreadObservation,
    connection_raw: &Value,
    first: usize,
    parent: &MetricsPrObservation,
    budget: &mut ThreadBudget,
    seen: &mut SeenComments,
    expected_total: &mut Option<u64>,
) -> GithubResult<Option<String>> {
    let page = connection(connection_raw, first)?;
    if expected_total.is_some_and(|total| total != page.total) {
        return Err(error(
            GithubErrorKind::StaleHead,
            "Nested comment total changed while paging; moving evidence is not complete",
        ));
    }
    *expected_total = Some(page.total);
    if page.nodes.len()
        > budget
            .limits
            .max_comments
            .saturating_sub(budget.raw_comments)
    {
        return Err(error(
            GithubErrorKind::Json,
            "Nested comment source exceeded aggregate raw comment budget",
        ));
    }
    let coverage = &mut target.comment_coverage;
    coverage.fetched_pages += 1;
    coverage.fetched_rows += page.nodes.len();
    budget.raw_comments += page.nodes.len();
    for raw in page.nodes {
        if doc_size(raw)? > budget.limits.max_document_bytes {
            coverage.excluded_rows += 1;
            coverage.metric_evidence_complete = false;
            coverage.reasons.push(
                "Comment document exceeds selected bytes; not admitted or treated absent".into(),
            );
            continue;
        }
        let item = comment(raw, parent)?;
        if !seen.nodes.insert(item.node_id.clone()) {
            return Err(error(GithubErrorKind::IdentityChanged, "Comment node repeated across pages/threads; no arbitrary original author or body selected"));
        }
        if let Some(id) = item.database_id {
            if seen.numeric.insert(id, item.node_id.clone()).is_some() {
                return Err(error(GithubErrorKind::IdentityChanged, "Actual positive comment database ID contradicts opaque-node membership across pages/threads"));
            }
        }
        if !item.reasons.is_empty() {
            coverage.metric_evidence_complete = false;
            coverage.reasons.extend(item.reasons.clone());
        }
        if matches!(item.reply_to, MetricsThreadReplyTo::Original) {
            if target.original_comment.is_some() {
                return Err(error(
                    GithubErrorKind::IdentityChanged,
                    "Thread has contradictory explicit original comment nodes",
                ));
            }
            target.original_comment = Some(MetricsOriginalCommentCandidate {
                comment_node_id: item.node_id.clone(),
                original_line: item.original_line,
                original_start_line: item.original_start_line,
                original_commit_sha: item.original_commit_sha.clone(),
                observed_path: item.observed_path.clone(),
            });
        }
        target.comments.push(item);
        coverage.admitted_rows += 1;
    }
    if page.next.is_none() {
        if coverage.fetched_rows as u64 != page.total {
            coverage.metric_evidence_complete = false;
            coverage.reasons.push(
                "Exhausted comment connection count contradicts actual raw rows; not complete"
                    .into(),
            );
        } else {
            coverage.enumeration_complete = true;
        }
    }
    Ok(page.next)
}
fn collect_comments(
    provider: &GithubProvider,
    parent: &MetricsPrObservation,
    target: &mut MetricsReviewThreadObservation,
    connection_raw: &Value,
    initial_first: usize,
    context: (&mut ThreadBudget, &mut SeenComments),
    cancel: &CancellationToken,
) -> GithubResult<()> {
    let (budget, seen) = context;
    let mut expected_total = None;
    let before_bytes = budget.bytes;
    if initial_first == 0 {
        if !connection_raw.is_null() {
            return Err(error(
                GithubErrorKind::Json,
                "Skipped nested connection returned unsolicited metadata/rows beyond pre-request budget",
            ));
        }
        target.comment_coverage.metric_evidence_complete = false;
        target.comment_coverage.reasons.push("Outer states retained with nested connection skipped after raw-comment budget; omitted comments are unknown, not empty".into());
        finalize(&mut target.comment_coverage);
        return Ok(());
    }
    target.comment_coverage.requested_pages = 1;
    let mut next = comments_page(
        target,
        connection_raw,
        initial_first,
        parent,
        budget,
        seen,
        &mut expected_total,
    )?;
    let mut cursors = BTreeSet::new();
    while let Some(cursor) = next {
        if target.comment_coverage.requested_pages >= budget.limits.max_comment_pages_per_thread
            || budget.graphql_pages >= budget.limits.max_graphql_pages
            || budget
                .raw_comments
                .saturating_add(budget.limits.comment_page_size)
                > budget.limits.max_comments
        {
            target.comment_coverage.metric_evidence_complete = false;
            target.comment_coverage.reasons.push("Nested fixed-page/comment/GraphQL budget reached before another request; unseen comments remain unknown".into());
            break;
        }
        if !cursors.insert(cursor.clone()) {
            return Err(error(
                GithubErrorKind::Json,
                "Nested thread comments repeated a pagination cursor",
            ));
        }
        target.comment_coverage.requested_pages += 1;
        let value = match budget.graphql(
            provider,
            METRICS_THREAD_COMMENTS_QUERY,
            json!({"id":target.node_id,"first":budget.limits.comment_page_size,"after":cursor}),
            cancel,
        ) {
            Ok(value) => value,
            Err(err) if private_failure(&err) => return Err(err),
            Err(err) => {
                target.comment_coverage.metric_evidence_complete = false;
                target.comment_coverage.reasons.push(err.message.clone());
                target.comment_coverage.status = MetricsReadStatus::Unavailable(err);
                break;
            }
        };
        let raw = node_value(&value, "PullRequestReviewThread")?;
        let fresh = thread(raw, parent)?;
        if fresh.node_id != target.node_id
            || fresh.subject_type != target.subject_type
            || fresh.resolved != target.resolved
            || fresh.outdated != target.outdated
            || fresh.placement != target.placement
            || fresh.resolved_by != target.resolved_by
            || fresh.viewer_can_reply != target.viewer_can_reply
            || fresh.viewer_can_resolve != target.viewer_can_resolve
            || fresh.viewer_can_unresolve != target.viewer_can_unresolve
        {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Thread state/placement/resolver moved during nested pagination",
            ));
        }
        next = comments_page(
            target,
            &raw["comments"],
            budget.limits.comment_page_size,
            parent,
            budget,
            seen,
            &mut expected_total,
        )?;
    }
    target.comment_coverage.response_bytes = budget.bytes - before_bytes;
    if !target.comment_coverage.enumeration_complete {
        target
            .comment_coverage
            .reasons
            .push("Nested comments were not exhausted; outer thread count is independent".into());
    }
    if target.comment_coverage.enumeration_complete && target.comment_coverage.excluded_rows == 0 {
        let nodes = target
            .comments
            .iter()
            .map(|comment| comment.node_id.as_str())
            .collect::<BTreeSet<_>>();
        if target.comments.iter().any(|comment| matches!(&comment.reply_to, MetricsThreadReplyTo::Reply(node) if !nodes.contains(node.as_str()))) {
            return Err(error(GithubErrorKind::IdentityChanged, "Exhausted thread reply references an absent/foreign comment node; no reply-role alias guessed"));
        }
    }
    finalize(&mut target.comment_coverage);
    Ok(())
}
fn provenance(
    threads: &[MetricsReviewThreadObservation],
    states_complete: bool,
) -> MetricsThreadProvenanceCoverage {
    let mut coverage = MetricsThreadProvenanceCoverage {
        comment_enumeration_complete: states_complete,
        actors_complete: states_complete,
        content_complete: states_complete,
        original_candidates_complete: states_complete,
        original_tree_verification_available: false,
        reasons: vec!["Observed original line/commit/path candidates require independent immutable old-tree verification; no original path/side is asserted".into()],
    };
    for thread in threads {
        coverage.comment_enumeration_complete &= thread.comment_coverage.enumeration_complete
            && thread.comment_coverage.excluded_rows == 0;
        coverage.actors_complete &= thread.comment_coverage.enumeration_complete
            && thread.comment_coverage.excluded_rows == 0
            && thread.comments.iter().all(|comment| {
                comment.author.as_ref().is_some_and(|actor| {
                    actor.account_id.is_some() && !actor.actor.login.is_empty()
                })
            })
            && (thread.resolved == Some(false)
                || thread.resolved_by.as_ref().is_some_and(|actor| {
                    actor.account_id.is_some() && !actor.actor.login.is_empty()
                }));
        coverage.content_complete &= thread.comment_coverage.excluded_rows == 0
            && thread.comments.iter().all(|comment| comment.body.is_some());
        coverage.original_candidates_complete &=
            thread.original_comment.as_ref().is_some_and(|original| {
                original.original_line.is_some()
                    && original.original_commit_sha.is_some()
                    && original.observed_path.is_some()
            });
        coverage.reasons.extend(thread.provenance_reasons.clone());
        coverage
            .reasons
            .extend(thread.comment_coverage.reasons.clone());
        if !thread.comment_coverage.enumeration_complete {
            coverage.actors_complete = false;
            coverage.content_complete = false;
            coverage.original_candidates_complete = false;
        }
    }
    coverage.reasons.sort();
    coverage.reasons.dedup();
    coverage
}
impl GithubProvider {
    /// Purpose-specific actual selected-node read for the thread publication
    /// worker. The caller owns the original source/account/local closure; this
    /// intermediate observation is not a permission or publication receipt.
    pub(crate) fn pr_selected_thread_observation_v1(
        &self,
        original: &PrResourceParentV1,
        node: &str,
        cancel: &CancellationToken,
        deadline: std::time::Instant,
    ) -> GithubResult<MetricsReviewThreadObservation> {
        if !opaque(node) || cancel.is_cancelled() || std::time::Instant::now() >= deadline {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Original thread node/phase unavailable",
            ));
        }
        let parent = self.pr_thread_resource_parent_v1(original, cancel)?;
        let limits = MetricsThreadLimits {
            max_thread_pages: 1,
            thread_page_size: 1,
            max_threads: 1,
            max_comment_pages_per_thread: 3,
            comment_page_size: 100,
            max_comments: 300,
            max_graphql_pages: 3,
            max_document_bytes: 128 * 1024,
            max_response_bytes: 4 * 1024 * 1024,
            max_duration: deadline.saturating_duration_since(std::time::Instant::now()),
        };
        let mut budget = ThreadBudget::new(&limits)?;
        let value = budget.graphql(
            self,
            METRICS_THREAD_COMMENTS_QUERY,
            json!({"id":node,"first":100,"after":null}),
            cancel,
        )?;
        let raw = node_value(&value, "PullRequestReviewThread")?;
        let mut actual = thread(raw, &parent)?;
        if actual.node_id != node {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Selected thread node changed",
            ));
        }
        collect_comments(
            self,
            &parent,
            &mut actual,
            &raw["comments"],
            100,
            (&mut budget, &mut SeenComments::default()),
            cancel,
        )?;
        if cancel.is_cancelled() || std::time::Instant::now() >= deadline {
            return Err(error(
                GithubErrorKind::Transport,
                "Original thread observation phase expired",
            ));
        }
        Ok(actual)
    }
    /// A genuine fresh REST producer for the native thread reader. Partial
    /// Files metadata is an expectation, never a fabricated Metrics DTO.
    pub(crate) fn pr_thread_resource_parent_v1(
        &self,
        original: &PrResourceParentV1,
        cancel: &CancellationToken,
    ) -> GithubResult<MetricsPrObservation> {
        let endpoint = format!("repos/{}", original.repository());
        let raw_repo = self.api(&endpoint, "GET", None, None, false, cancel)?;
        let repo = super::parse::repository(&self.host, original.repository(), &raw_repo)?;
        if repo.id != original.repository_id() || repo.node_id != original.repository_node() {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Original thread repository was replaced",
            ));
        }
        let raw = self.api(
            &format!("{endpoint}/pulls/{}", original.number()),
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        let parent = super::parse::parse_pr(&repo, &raw)?;
        if parent.id != original.database_id()
            || parent.node_id != original.node_id()
            || parent.number != original.number()
            || parent.head_sha.as_deref() != Some(original.head_sha())
            || raw["base"]["sha"].as_str() != Some(original.base_tip_sha())
        {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Original thread parent or comparison changed",
            ));
        }
        Ok(parent)
    }
    /// Fresh current source evidence only: user/repository/PR/GraphQL pages/
    /// repository/PR/user in one budget (minimum7), no source generation/history.
    pub fn metrics_review_threads(
        &self,
        identity: &HostIdentity,
        original: &MetricsPrObservation,
        limits: &MetricsThreadLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<MetricsReviewThreadsSnapshot> {
        scope(self, identity, &original.repository.full_name)?;
        limits.validate()?;
        if original.repository.host != self.host
            || original.repository.full_name != original.repository.full_name.to_ascii_lowercase()
            || original.repository.id == 0
            || original.id == 0
            || original.number == 0
            || !opaque(&original.repository.node_id)
            || !opaque(&original.node_id)
            || original
                .head_sha
                .as_ref()
                .is_none_or(|head| !crate::search::model::valid_sha(head))
        {
            return Err(error(GithubErrorKind::InvalidTarget, "Thread evidence requires positive exact repository/PR database+node identities and original full head"));
        }
        let mut budget = ThreadBudget::new(limits)?;
        budget.principal(self, identity, cancel)?;
        let repo_endpoint = format!("repos/{}", original.repository.full_name);
        let parent_endpoint = format!("{repo_endpoint}/pulls/{}", original.number);
        let repo = repository(
            &self.host,
            &original.repository.full_name,
            &budget.get(self, &repo_endpoint, cancel)?,
        )?;
        require_same_repo(&original.repository, &repo)?;
        let parent = parse_pr(&repo, &budget.get(self, &parent_endpoint, cancel)?)?;
        require_same_parent(original, &parent)?;
        let mut threads = vec![];
        let mut seen_threads = BTreeSet::new();
        let mut seen_comments = SeenComments::default();
        let mut coverage = empty_coverage();
        let mut after: Option<String> = None;
        let mut cursors = BTreeSet::new();
        let mut total = None;
        let before_bytes = budget.bytes;
        for _ in 0..limits.max_thread_pages {
            if coverage
                .fetched_rows
                .saturating_add(limits.thread_page_size)
                > limits.max_threads
                || budget.graphql_pages >= limits.max_graphql_pages
            {
                coverage.reasons.push(
                    "Outer fixed-page/raw-thread/GraphQL budget reached before another request"
                        .into(),
                );
                break;
            }
            let include_comments = limits
                .thread_page_size
                .saturating_mul(limits.comment_page_size)
                <= limits.max_comments.saturating_sub(budget.raw_comments);
            coverage.requested_pages += 1;
            let value = match budget.graphql(self, METRICS_REVIEW_THREADS_QUERY, json!({"id":parent.node_id,"first":limits.thread_page_size,"after":after,"commentsFirst":limits.comment_page_size,"includeComments":include_comments}), cancel) {
                Ok(value) => value,
                Err(err) if private_failure(&err) => return Err(err),
                Err(err) => {
                    coverage.metric_evidence_complete = false;
                    coverage.reasons.push(err.message.clone());
                    coverage.status = MetricsReadStatus::Unavailable(err);
                    break;
                }
            };
            let raw_parent = node_value(&value, "PullRequest")?;
            graph_parent(raw_parent, &parent)?;
            let page = connection(&raw_parent["reviewThreads"], limits.thread_page_size)?;
            if total.is_some_and(|total| total != page.total) {
                return Err(error(
                    GithubErrorKind::StaleHead,
                    "Outer thread total moved during pagination",
                ));
            }
            total = Some(page.total);
            coverage.fetched_pages += 1;
            coverage.fetched_rows += page.nodes.len();
            for raw in page.nodes {
                let mut target = thread(raw, &parent)?;
                if !seen_threads.insert(target.node_id.clone()) {
                    return Err(error(
                        GithubErrorKind::IdentityChanged,
                        "Thread node repeated across pages; no duplicate/contradictory state count",
                    ));
                }
                if !target.state_reasons.is_empty() {
                    coverage.metric_evidence_complete = false;
                    coverage.reasons.extend(target.state_reasons.clone());
                }
                collect_comments(
                    self,
                    &parent,
                    &mut target,
                    &raw["comments"],
                    if include_comments {
                        limits.comment_page_size
                    } else {
                        0
                    },
                    (&mut budget, &mut seen_comments),
                    cancel,
                )?;
                threads.push(target);
                coverage.admitted_rows += 1;
            }
            match page.next {
                None => {
                    if coverage.fetched_rows as u64 == page.total {
                        coverage.enumeration_complete = true;
                    } else {
                        coverage.metric_evidence_complete = false;
                        coverage.reasons.push("Exhausted outer thread total contradicts actual rows; no complete count".into());
                    }
                    break;
                }
                Some(cursor) => {
                    if !cursors.insert(cursor.clone()) {
                        return Err(error(
                            GithubErrorKind::Json,
                            "Outer thread connection repeated a cursor",
                        ));
                    }
                    after = Some(cursor);
                }
            }
        }
        coverage.response_bytes = budget.bytes - before_bytes;
        if !coverage.enumeration_complete {
            coverage
                .reasons
                .push("Outer thread states not exhausted; unseen threads remain unknown".into());
        }
        finalize(&mut coverage);
        let final_repo = repository(
            &self.host,
            &repo.full_name,
            &budget.get(self, &repo_endpoint, cancel)?,
        )?;
        require_same_repo(&repo, &final_repo)?;
        let final_parent = parse_pr(&repo, &budget.get(self, &parent_endpoint, cancel)?)?;
        require_same_parent(&parent, &final_parent)?;
        budget.principal(self, identity, cancel)?;
        Ok(MetricsReviewThreadsSnapshot {
            identity: identity.clone(),
            parent: final_parent,
            provenance_coverage: provenance(&threads, coverage.enumeration_complete),
            threads,
            state_coverage: coverage,
            observed_from: budget.observed_from,
            observed_through: budget.through()?,
            requests: budget.requests,
            graphql_pages: budget.graphql_pages,
            raw_comments: budget.raw_comments,
            aggregate_response_bytes: budget.bytes,
            successful_diagnostic_bytes: budget.successful_diagnostic_bytes,
            failed_reserved_bytes: budget.failed_reserved_bytes,
        })
    }
}
