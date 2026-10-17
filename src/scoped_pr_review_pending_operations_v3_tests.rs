//! Genuine original-source producer declarations; UNRUN until Root executes.
//! This child reuses real image/Git fixtures, not decoded pending records.
use super::*;
use crate::providers::gh::{
    PendingCommentMutationV3, PendingCommentOriginalV3, PendingCommentReceiptV3,
    PendingReviewOriginalV3,
};
use std::time::Duration;

fn graph_parent_v3(h: &history::GitImagesV1) -> Value {
    let raw = current_parent(h);
    let repository = repo(410, REPO);
    json!({"id":raw["node_id"],"fullDatabaseId":raw["id"],"number":7,
        "headRefOid":h.h,"baseRefOid":h.b,"repository":{
            "id":repository["node_id"],"databaseId":410,"nameWithOwner":REPO}})
}
pub(super) fn graph_review_v3(h: &history::GitImagesV1) -> Value {
    let parent = graph_parent_v3(h);
    json!({"__typename":"PullRequestReview","id":"ACTUAL_REVIEW_810","fullDatabaseId":"810",
        "body":"pending summary","state":"PENDING","commit":{"oid":h.h},
        "author":{"__typename":"User","databaseId":42,"login":"not-an-authorization-key"},
        "pullRequest":parent,"repository":parent["repository"]})
}
pub(super) fn graph_comment_v3(h: &history::GitImagesV1) -> Value {
    let parent = graph_parent_v3(h);
    json!({"__typename":"PullRequestReviewComment","id":"ACTUAL_COMMENT_910","fullDatabaseId":"910",
        "body":"original LEFT draft","state":"PENDING","path":history::PATH,"line":2,"startLine":1,
        "commit":{"oid":h.h},"updatedAt":"2026-10-15T09:00:00Z",
        "author":{"__typename":"User","databaseId":42,"login":"renamed-owner"},
        "viewerCanUpdate":true,"viewerCanDelete":true,
        "pullRequestReview":{"id":"ACTUAL_REVIEW_810","fullDatabaseId":"810","state":"PENDING"},
        "pullRequest":parent,"repository":parent["repository"]})
}
fn graph_step_v3(query: &str, id: &str, node: Value) -> GhStep {
    write(
        "graphql",
        "POST",
        Some(json!({"query":query,"variables":{"id":id}})),
        json!({"data":{"node":node}}),
    )
}
const REVIEW_QUERY_V3:&str="query($id:ID!){node(id:$id){__typename ... on PullRequestReview{id fullDatabaseId body state commit{oid} author{__typename login ... on User{databaseId} ... on Bot{databaseId}} pullRequest{id fullDatabaseId number headRefOid baseRefOid repository{id databaseId nameWithOwner}} repository{id databaseId nameWithOwner}}}}";
const COMMENT_QUERY_V3:&str="query($id:ID!){node(id:$id){__typename ... on PullRequestReviewComment{id fullDatabaseId body state path line startLine commit{oid} updatedAt author{__typename login ... on User{databaseId} ... on Bot{databaseId}} viewerCanUpdate viewerCanDelete pullRequestReview{id fullDatabaseId state} pullRequest{id fullDatabaseId number headRefOid baseRefOid repository{id databaseId nameWithOwner}} repository{id databaseId nameWithOwner}}}}";
pub(super) fn source_steps_v3(
    h: &history::GitImagesV1,
    review_node: Value,
    comment_node: Option<Value>,
) -> Vec<GhStep> {
    let mut steps = opening_steps(h);
    let row = review(h, "PENDING", "pending summary", 42);
    steps.extend(snapshot_steps(h, &row));
    steps.push(graph_step_v3(
        REVIEW_QUERY_V3,
        "ACTUAL_REVIEW_810",
        review_node,
    ));
    if let Some(comment) = comment_node {
        steps.push(graph_step_v3(
            COMMENT_QUERY_V3,
            "ACTUAL_COMMENT_910",
            comment,
        ));
    }
    steps.extend(closing_steps(h));
    steps
}
type OriginalPendingPairV3 = (
    Arc<PendingReviewOriginalV3>,
    Result<Arc<PendingCommentOriginalV3>, crate::providers::gh::GithubError>,
);
fn original_v3(
    s: &Setup,
    proof: &Arc<ClosedPrFileImagesV1>,
    selected: Arc<crate::providers::gh::ReviewSnapshotV2>,
    wrong_token: bool,
) -> Result<OriginalPendingPairV3, crate::providers::gh::GithubError> {
    let provider = provider(&s.fixture);
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let path = s.fixture.root.join("cache.sqlite3");
    let mut session = provider.begin_repository_catalogue_access(
        proof.source.catalogue_source().unwrap().source(),
        &Default::default(),
        &cancel,
    )?;
    let window =
        session.reserve_measured_external_reads(8, 1024 * 1024, 8 * 1024 * 1024, 2, &cancel)?;
    let measured = provider.clone().with_measured_read_window(window)?;
    session.verify(proof.navigation.original(), &cancel)?;
    let parent = measured.pr_inline_original_parent_v1(
        session
            .observation_v1(proof.navigation.original().id)
            .unwrap(),
        s.original_files.observed(),
        &Default::default(),
        &cancel,
    )?;
    let local = ResourceLocalGuard::capture(
        &path,
        proof.source.catalogue_source().unwrap(),
        session.identity(),
        parent.summary(),
    )?;
    let source = measured.pending_review_original_v3(
        session.identity(),
        &parent,
        &selected,
        &Default::default(),
        &cancel,
        deadline,
    )?;
    let sibling = CancellationToken::default();
    let member = measured.pending_comment_original_v3(
        source.clone(),
        0,
        if wrong_token { &sibling } else { &cancel },
        deadline,
    );
    measured.pr_inline_original_parent_v1(
        session
            .observation_v1(proof.navigation.original().id)
            .unwrap(),
        s.original_files.observed(),
        &Default::default(),
        &cancel,
    )?;
    session.verify(proof.navigation.original(), &cancel)?;
    session.close_with_guard(&cancel, || {
        local.validate(&path, proof.source.catalogue_source().unwrap())
    })?;
    Ok((source, member))
}

#[test]
fn pending_reviews_v3_source_actual_owned_pending_numeric_membership_and_capabilities_never_uses_submitted_factory(
) {
    let mut s = inline_setup(|h, steps| {
        steps.extend(select_steps(
            h,
            &review(h, "PENDING", "pending summary", 42),
        ));
        steps.extend(source_steps_v3(
            h,
            graph_review_v3(h),
            Some(graph_comment_v3(h)),
        ));
    });
    let loaded = read(&mut s, &Default::default(), &Default::default()).unwrap();
    let selected = Arc::new(select_pending(&s, &loaded.proof));
    let before = commands(&s.fixture).len();
    let sql = sql_state(&s.fixture);
    let (source, member) = original_v3(&s, &loaded.proof, selected, false).unwrap();
    let member = member.unwrap();
    assert_eq!(source.principal(), 42);
    assert!(source.has_current_commit());
    assert_eq!(source.snapshot().review().id(), 810);
    assert_eq!(member.comment().id(), 910);
    assert_eq!(member.comment().author_account_id(), Some(42));
    assert_eq!(member.comment().body(), "original LEFT draft");
    assert_eq!(member.viewer_can_update(), Some(true));
    assert_eq!(member.viewer_can_delete(), Some(true));
    assert_eq!(member.updated_at(), "2026-10-15T09:00:00Z");
    assert!(commands(&s.fixture).len() > before);
    assert_eq!(sql_state(&s.fixture), sql);
    unchanged_git_and_exact_script(&s);
}

#[test]
fn pending_reviews_v3_source_submitted_foreign_or_missing_actual_pending_state_never_becomes_edit_permission(
) {
    for failure in [
        "submitted",
        "foreign",
        "missing",
        "wrong-review",
        "wrong-body",
        "invalid-time",
    ] {
        let mut s = inline_setup(|h, steps| {
            steps.extend(select_steps(
                h,
                &review(h, "PENDING", "pending summary", 42),
            ));
            let mut node = graph_comment_v3(h);
            match failure {
                "submitted" => node["state"] = json!("SUBMITTED"),
                "foreign" => node["author"]["databaseId"] = json!(43),
                "missing" => node["state"] = Value::Null,
                "wrong-review" => node["pullRequestReview"]["fullDatabaseId"] = json!(811),
                "wrong-body" => node["body"] = json!("concurrent replacement"),
                "invalid-time" => node["updatedAt"] = json!("not-a-time"),
                _ => unreachable!(),
            }
            steps.extend(source_steps_v3(h, graph_review_v3(h), Some(node)));
        });
        let loaded = read(&mut s, &Default::default(), &Default::default()).unwrap();
        let selected = Arc::new(select_pending(&s, &loaded.proof));
        let sql = sql_state(&s.fixture);
        let (_, member) = original_v3(&s, &loaded.proof, selected, false).unwrap();
        assert!(member.is_err());
        assert_eq!(sql_state(&s.fixture), sql);
        unchanged_git_and_exact_script(&s);
    }
}

#[test]
fn pending_reviews_v3_source_sibling_token_cannot_query_or_adopt_actual_same_producer_pending_member(
) {
    let mut s = inline_setup(|h, steps| {
        steps.extend(select_steps(
            h,
            &review(h, "PENDING", "pending summary", 42),
        ));
        steps.extend(source_steps_v3(h, graph_review_v3(h), None));
    });
    let loaded = read(&mut s, &Default::default(), &Default::default()).unwrap();
    let selected = Arc::new(select_pending(&s, &loaded.proof));
    let sql = sql_state(&s.fixture);
    let (_, member) = original_v3(&s, &loaded.proof, selected, true).unwrap();
    assert!(member.is_err());
    assert_eq!(sql_state(&s.fixture), sql);
    unchanged_git_and_exact_script(&s);
}

#[test]
fn pending_reviews_v3_source_unknown_comment_capability_is_observed_unknown_not_same_user_or_push_authority(
) {
    let mut s = inline_setup(|h, steps| {
        steps.extend(select_steps(
            h,
            &review(h, "PENDING", "pending summary", 42),
        ));
        let mut node = graph_comment_v3(h);
        node["viewerCanUpdate"] = Value::Null;
        node["viewerCanDelete"] = Value::Null;
        steps.extend(source_steps_v3(h, graph_review_v3(h), Some(node)));
    });
    let loaded = read(&mut s, &Default::default(), &Default::default()).unwrap();
    let selected = Arc::new(select_pending(&s, &loaded.proof));
    let sql = sql_state(&s.fixture);
    let (_, member) = original_v3(&s, &loaded.proof, selected, false).unwrap();
    let member = member.unwrap();
    assert_eq!(member.viewer_can_update(), None);
    assert_eq!(member.viewer_can_delete(), None);
    assert_eq!(member.comment().author_account_id(), Some(42));
    assert_eq!(sql_state(&s.fixture), sql);
    unchanged_git_and_exact_script(&s);
}

#[derive(Clone, Copy)]
enum MutationV3 {
    Add,
    Edit,
    Delete,
    CoordinateOverflow,
}
const RETURN_FIELDS_V3:&str="__typename id fullDatabaseId body state path line startLine commit{oid} updatedAt author{__typename login ... on User{databaseId} ... on Bot{databaseId}} pullRequestReview{id fullDatabaseId state} pullRequest{id fullDatabaseId number headRefOid baseRefOid repository{id databaseId nameWithOwner}} repository{id databaseId nameWithOwner}";
fn mutation_payload_v3(kind: MutationV3) -> Value {
    let(input_type,operation,input,fields)=match kind {
        MutationV3::Add|MutationV3::CoordinateOverflow=>("AddPullRequestReviewThreadInput","addPullRequestReviewThread",
            json!({"pullRequestReviewId":"ACTUAL_REVIEW_810","path":history::PATH,"line":2,"side":"LEFT",
                "subjectType":"LINE","body":"additional pending body","startLine":1,"startSide":"LEFT"}),
            format!("thread{{id comments(first:2){{nodes{{{RETURN_FIELDS_V3}}} totalCount pageInfo{{hasNextPage}}}}}}")),
        MutationV3::Edit=>("UpdatePullRequestReviewCommentInput","updatePullRequestReviewComment",
            json!({"pullRequestReviewCommentId":"ACTUAL_COMMENT_910","body":"edited pending body"}),
            format!("pullRequestReviewComment{{{RETURN_FIELDS_V3}}}")),
        MutationV3::Delete=>("DeletePullRequestReviewCommentInput","deletePullRequestReviewComment",
            json!({"id":"ACTUAL_COMMENT_910"}),"pullRequestReview{id fullDatabaseId state} pullRequestReviewComment{id fullDatabaseId}".into()),
    };
    json!({"query":format!("mutation($input:{input_type}!){{{operation}(input:$input){{{fields}}}}}"),"variables":{"input":input}})
}
fn mutation_source_steps_v3(
    h: &history::GitImagesV1,
    kind: MutationV3,
    capability: Option<bool>,
) -> Vec<GhStep> {
    let mut steps = snapshot_steps(h, &review(h, "PENDING", "pending summary", 42));
    steps.push(graph_step_v3(
        REVIEW_QUERY_V3,
        "ACTUAL_REVIEW_810",
        graph_review_v3(h),
    ));
    if matches!(kind, MutationV3::Edit | MutationV3::Delete) {
        let mut node = graph_comment_v3(h);
        node["viewerCanUpdate"] = json!(capability);
        node["viewerCanDelete"] = json!(capability);
        steps.push(graph_step_v3(COMMENT_QUERY_V3, "ACTUAL_COMMENT_910", node));
    }
    steps
}
fn mutation_steps_v3(
    h: &history::GitImagesV1,
    kind: MutationV3,
    null_delete_parent: bool,
) -> Vec<GhStep> {
    let mut steps = opening_steps(h);
    steps.extend(mutation_source_steps_v3(h, kind, Some(true)));
    if matches!(kind, MutationV3::CoordinateOverflow) {
        return steps;
    }
    steps.extend(guard_steps(h));
    steps.extend(mutation_source_steps_v3(h, kind, Some(true)));
    if matches!(kind, MutationV3::Add) {
        steps.push(get(
            &format!("repos/{REPO}/pulls/7/files?per_page=100&page=1"),
            json!([h.file()]),
        ));
    }
    steps.push(get(&format!("repos/{REPO}/pulls/7"), current_parent(h)));
    steps.extend(closing_steps(h));
    let mut graph = graph_comment_v3(h);
    let response = match kind {
        MutationV3::Add => {
            graph["id"] = json!("ACTUAL_COMMENT_912");
            graph["fullDatabaseId"] = json!("912");
            graph["body"] = json!("additional pending body");
            json!({"data":{"addPullRequestReviewThread":{"thread":{"id":"ACTUAL_PENDING_THREAD_912",
                "comments":{"nodes":[graph],"totalCount":1,"pageInfo":{"hasNextPage":false}}}}}})
        }
        MutationV3::Edit => {
            graph["body"] = json!("edited pending body");
            json!({"data":{"updatePullRequestReviewComment":{"pullRequestReviewComment":graph}}})
        }
        MutationV3::Delete => json!({"data":{"deletePullRequestReviewComment":{
            "pullRequestReview":if null_delete_parent{Value::Null}else{json!({"id":"ACTUAL_REVIEW_810","fullDatabaseId":"810","state":"PENDING"})},
            "pullRequestReviewComment":{"id":"ACTUAL_COMMENT_910","fullDatabaseId":"910"}}}}),
        MutationV3::CoordinateOverflow => unreachable!(),
    };
    steps.push(write(
        "graphql",
        "POST",
        Some(mutation_payload_v3(kind)),
        response,
    ));
    if null_delete_parent {
        return steps;
    }
    let mut actual = comments(h);
    match kind {
        MutationV3::Add => {
            let mut added = actual[0].clone();
            added["id"] = json!(912);
            added["node_id"] = json!("ACTUAL_COMMENT_912");
            added["body"] = json!("additional pending body");
            actual.push(added);
        }
        MutationV3::Edit => actual[0]["body"] = json!("edited pending body"),
        MutationV3::Delete => {
            actual.remove(0);
        }
        MutationV3::CoordinateOverflow => unreachable!(),
    }
    let row = review(h, "PENDING", "pending summary", 42);
    steps.push(get(
        &format!("repos/{REPO}/pulls/7/reviews/810"),
        row.clone(),
    ));
    steps.push(get(
        &format!("repos/{REPO}/pulls/7/reviews/810/comments?per_page=100&page=1"),
        json!(actual),
    ));
    steps.push(get(&format!("repos/{REPO}/pulls/7/reviews/810"), row));
    if matches!(kind, MutationV3::Delete) {
        steps.push(write("graphql","POST",Some(json!({"query":"query($id:ID!){node(id:$id){__typename ... on PullRequestReviewComment{id}}}",
            "variables":{"id":"ACTUAL_COMMENT_910"}})),json!({"data":{"node":null}})));
    }
    steps
}
fn publish_comment_v3(
    s: &Setup,
    proof: &Arc<ClosedPrFileImagesV1>,
    selected: &Arc<crate::providers::gh::ReviewSnapshotV2>,
    kind: MutationV3,
    callback: &AtomicUsize,
) -> Result<PendingCommentReceiptV3, ReviewPublicationFailureV2> {
    let provider = provider(&s.fixture);
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let path = s.fixture.root.join("cache.sqlite3");
    let mut session = provider
        .begin_repository_catalogue_access(
            proof.source.catalogue_source().unwrap().source(),
            &Default::default(),
            &cancel,
        )
        .map_err(ReviewPublicationFailureV2::NotStarted)?;
    let limits = ReviewReadLimitsV2::default();
    let caps = vec![1024 * 1024; limits.max_pages + 4];
    let mut captures = session
        .reserve_postwrite_capture_v2(&caps, 2, &cancel)
        .map_err(ReviewPublicationFailureV2::NotStarted)?;
    let window = session
        .reserve_measured_external_reads(32, 1024 * 1024, 8 * 1024 * 1024, 2, &cancel)
        .map_err(ReviewPublicationFailureV2::NotStarted)?;
    let measured = provider
        .clone()
        .with_measured_read_window(window)
        .map_err(ReviewPublicationFailureV2::NotStarted)?;
    session
        .verify(proof.navigation.original(), &cancel)
        .map_err(ReviewPublicationFailureV2::NotStarted)?;
    let parent = measured
        .pr_inline_original_parent_v1(
            session
                .observation_v1(proof.navigation.original().id)
                .unwrap(),
            s.original_files.observed(),
            &Default::default(),
            &cancel,
        )
        .map_err(ReviewPublicationFailureV2::NotStarted)?;
    let identity = session.identity().clone();
    let local = ResourceLocalGuard::capture(
        &path,
        proof.source.catalogue_source().unwrap(),
        &identity,
        parent.summary(),
    )
    .map_err(ReviewPublicationFailureV2::NotStarted)?;
    let review = measured
        .pending_review_original_v3(&identity, &parent, selected, &limits, &cancel, deadline)
        .map_err(ReviewPublicationFailureV2::NotStarted)?;
    let member = if matches!(kind, MutationV3::Edit | MutationV3::Delete) {
        Some(
            measured
                .pending_comment_original_v3(review.clone(), 0, &cancel, deadline)
                .map_err(ReviewPublicationFailureV2::NotStarted)?,
        )
    } else {
        None
    };
    let mut inline = requested_comments().remove(0);
    inline.body = "additional pending body".into();
    if matches!(kind, MutationV3::CoordinateOverflow) {
        inline.line = i32::MAX as u64 + 1;
        inline.start_line = None;
        inline.start_side = None;
    }
    let mutation = match kind {
        MutationV3::Add | MutationV3::CoordinateOverflow => PendingCommentMutationV3::Add {
            review: &review,
            comment: &inline,
        },
        MutationV3::Edit => PendingCommentMutationV3::Edit {
            comment: member.as_ref().unwrap(),
            body: "edited pending body",
        },
        MutationV3::Delete => PendingCommentMutationV3::Delete {
            comment: member.as_ref().unwrap(),
        },
    };
    provider.mutate_pending_comment_guarded_v3(
        &measured,
        &mut captures,
        &identity,
        &parent,
        mutation,
        mutation,
        &limits,
        &cancel,
        deadline,
        || {
            callback.fetch_add(1, Ordering::SeqCst);
            measured.pr_inline_original_parent_v1(
                session
                    .observation_v1(proof.navigation.original().id)
                    .unwrap(),
                s.original_files.observed(),
                &Default::default(),
                &cancel,
            )?;
            session.verify(proof.navigation.original(), &cancel)?;
            session.close_with_guard(&cancel, || {
                local.validate(&path, proof.source.catalogue_source().unwrap())
            })?;
            Ok(())
        },
    )
}

#[test]
fn pending_reviews_v3_provider_add_modern_pending_thread_one_root_and_all_untouched_comments_read_back(
) {
    let mut s = inline_setup(|h, steps| {
        steps.extend(select_steps(
            h,
            &review(h, "PENDING", "pending summary", 42),
        ));
        steps.extend(mutation_steps_v3(h, MutationV3::Add, false));
    });
    let loaded = read(&mut s, &Default::default(), &Default::default()).unwrap();
    let selected = Arc::new(select_pending(&s, &loaded.proof));
    let sql = sql_state(&s.fixture);
    let callback = AtomicUsize::new(0);
    let receipt =
        publish_comment_v3(&s, &loaded.proof, &selected, MutationV3::Add, &callback).unwrap();
    assert_eq!(callback.load(Ordering::SeqCst), 1);
    assert_eq!(receipt.comment_id(), 912);
    assert!(!receipt.deleted());
    assert_eq!(receipt.snapshot().unwrap().comments().len(), 3);
    assert_eq!(
        receipt.snapshot().unwrap().review().state(),
        &ObservedReviewStateV2::Pending
    );
    assert_eq!(sql_state(&s.fixture), sql);
    unchanged_git_and_exact_script(&s);
}
#[test]
fn pending_reviews_v3_provider_edit_exact_pending_identity_body_and_all_other_comments_preserved() {
    let mut s = inline_setup(|h, steps| {
        steps.extend(select_steps(
            h,
            &review(h, "PENDING", "pending summary", 42),
        ));
        steps.extend(mutation_steps_v3(h, MutationV3::Edit, false));
    });
    let loaded = read(&mut s, &Default::default(), &Default::default()).unwrap();
    let selected = Arc::new(select_pending(&s, &loaded.proof));
    let sql = sql_state(&s.fixture);
    let callback = AtomicUsize::new(0);
    let receipt =
        publish_comment_v3(&s, &loaded.proof, &selected, MutationV3::Edit, &callback).unwrap();
    assert_eq!(callback.load(Ordering::SeqCst), 1);
    assert_eq!(receipt.comment_id(), 910);
    assert!(!receipt.deleted());
    assert_eq!(
        receipt.snapshot().unwrap().comments()[0].body(),
        "edited pending body"
    );
    assert_eq!(
        receipt.snapshot().unwrap().comments()[1].body(),
        "original RIGHT draft"
    );
    assert_eq!(sql_state(&s.fixture), sql);
    unchanged_git_and_exact_script(&s);
}
#[test]
fn pending_reviews_v3_provider_delete_exact_pending_comment_requires_remaining_parent_and_explicit_node_absence(
) {
    let mut s = inline_setup(|h, steps| {
        steps.extend(select_steps(
            h,
            &review(h, "PENDING", "pending summary", 42),
        ));
        steps.extend(mutation_steps_v3(h, MutationV3::Delete, false));
    });
    let loaded = read(&mut s, &Default::default(), &Default::default()).unwrap();
    let selected = Arc::new(select_pending(&s, &loaded.proof));
    let sql = sql_state(&s.fixture);
    let callback = AtomicUsize::new(0);
    let receipt =
        publish_comment_v3(&s, &loaded.proof, &selected, MutationV3::Delete, &callback).unwrap();
    assert_eq!(callback.load(Ordering::SeqCst), 1);
    assert_eq!(receipt.comment_id(), 910);
    assert!(receipt.deleted());
    assert_eq!(receipt.snapshot().unwrap().comments().len(), 1);
    assert_eq!(receipt.snapshot().unwrap().comments()[0].id(), 911);
    assert_eq!(sql_state(&s.fixture), sql);
    unchanged_git_and_exact_script(&s);
}
#[test]
fn pending_reviews_v3_provider_nullable_deleted_parent_without_actual_comment_ack_never_infers_absence(
) {
    let mut s = inline_setup(|h, steps| {
        steps.extend(select_steps(
            h,
            &review(h, "PENDING", "pending summary", 42),
        ));
        let mut mutation = mutation_steps_v3(h, MutationV3::Delete, true);
        let last = mutation.last_mut().unwrap();
        let mut response: Value = serde_json::from_slice(&last.stdout).unwrap();
        response["data"]["deletePullRequestReviewComment"]["pullRequestReviewComment"] =
            Value::Null;
        last.stdout = serde_json::to_vec(&response).unwrap();
        steps.extend(mutation);
    });
    let loaded = read(&mut s, &Default::default(), &Default::default()).unwrap();
    let selected = Arc::new(select_pending(&s, &loaded.proof));
    let sql = sql_state(&s.fixture);
    let callback = AtomicUsize::new(0);
    assert!(matches!(
        publish_comment_v3(&s, &loaded.proof, &selected, MutationV3::Delete, &callback),
        Err(ReviewPublicationFailureV2::WriteMayHaveStarted(_))
    ));
    assert_eq!(callback.load(Ordering::SeqCst), 1);
    assert_eq!(sql_state(&s.fixture), sql);
    unchanged_git_and_exact_script(&s);
}
#[test]
fn pending_reviews_v3_provider_signed_coordinate_overflow_refuses_before_guard_or_post_no_downcast()
{
    let mut s = inline_setup(|h, steps| {
        steps.extend(select_steps(
            h,
            &review(h, "PENDING", "pending summary", 42),
        ));
        steps.extend(mutation_steps_v3(h, MutationV3::CoordinateOverflow, false));
    });
    let loaded = read(&mut s, &Default::default(), &Default::default()).unwrap();
    let selected = Arc::new(select_pending(&s, &loaded.proof));
    let sql = sql_state(&s.fixture);
    let callback = AtomicUsize::new(0);
    assert!(matches!(
        publish_comment_v3(
            &s,
            &loaded.proof,
            &selected,
            MutationV3::CoordinateOverflow,
            &callback
        ),
        Err(ReviewPublicationFailureV2::NotStarted(_))
    ));
    assert_eq!(callback.load(Ordering::SeqCst), 0);
    assert_eq!(sql_state(&s.fixture), sql);
    unchanged_git_and_exact_script(&s);
}

/// Transform only actual response fixtures; all factories still execute their
/// original installed-gh transport/parser and original source closure.
fn last_comment_responses_v3(steps: &mut [GhStep]) {
    for step in steps {
        let Ok(mut response) = serde_json::from_slice::<Value>(&step.stdout) else {
            continue;
        };
        if let Some(rows) = response.as_array_mut() {
            if rows
                .first()
                .is_some_and(|row| row.get("pull_request_review_id").is_some())
            {
                rows.truncate(1);
            }
        }
        if response.get("state").and_then(Value::as_str) == Some("PENDING")
            && response.get("commit_id").is_some()
        {
            response["body"] = json!("");
        }
        if response["data"]["node"]["__typename"].as_str() == Some("PullRequestReview") {
            response["data"]["node"]["body"] = json!("");
        }
        step.stdout = serde_json::to_vec(&response).unwrap();
    }
}
fn last_comment_steps_v3(h: &history::GitImagesV1, failure: Option<&str>) -> Vec<GhStep> {
    let mut steps = mutation_steps_v3(h, MutationV3::Delete, true);
    last_comment_responses_v3(&mut steps);
    let reviews = if failure == Some("catalogue-survivor") {
        json!([review(h, "PENDING", "", 42)])
    } else {
        json!([])
    };
    steps.push(catalogue_step(reviews));
    if failure == Some("catalogue-survivor") {
        return steps;
    }
    const REVIEW_ABSENCE: &str =
        "query($id:ID!){node(id:$id){__typename ... on PullRequestReview{id}}}";
    const COMMENT_ABSENCE: &str =
        "query($id:ID!){node(id:$id){__typename ... on PullRequestReviewComment{id}}}";
    let review_node = if failure == Some("review-node-survivor") {
        json!({"__typename":"PullRequestReview","id":"ACTUAL_REVIEW_810"})
    } else {
        Value::Null
    };
    let mut review_step = graph_step_v3(REVIEW_ABSENCE, "ACTUAL_REVIEW_810", review_node);
    if failure == Some("review-errors") {
        review_step.stdout = serde_json::to_vec(
            &json!({"data":{"node":null},"errors":[{"message":"Permission denied"}]}),
        )
        .unwrap();
    }
    steps.push(review_step);
    if matches!(failure, Some("review-node-survivor" | "review-errors")) {
        return steps;
    }
    let comment_node = if failure == Some("comment-node-survivor") {
        json!({"__typename":"PullRequestReviewComment","id":"ACTUAL_COMMENT_910"})
    } else {
        Value::Null
    };
    steps.push(graph_step_v3(
        COMMENT_ABSENCE,
        "ACTUAL_COMMENT_910",
        comment_node,
    ));
    steps
}

#[test]
fn pending_reviews_v3_provider_last_comment_empty_summary_observes_both_absent_without_fabricating_discard_or_snapshot(
) {
    let mut s = inline_setup(|h, steps| {
        let mut select = select_steps(h, &review(h, "PENDING", "", 42));
        last_comment_responses_v3(&mut select);
        steps.extend(select);
        steps.extend(last_comment_steps_v3(h, None));
    });
    let loaded = read(&mut s, &Default::default(), &Default::default()).unwrap();
    let selected = Arc::new(select_pending(&s, &loaded.proof));
    assert_eq!(selected.comments().len(), 1);
    assert!(selected.review().body().is_empty());
    let sql = sql_state(&s.fixture);
    let callback = AtomicUsize::new(0);
    let receipt =
        publish_comment_v3(&s, &loaded.proof, &selected, MutationV3::Delete, &callback).unwrap();
    assert_eq!(callback.load(Ordering::SeqCst), 1);
    assert_eq!(receipt.original_review_id(), 810);
    assert_eq!(receipt.comment_id(), 910);
    assert!(receipt.deleted());
    assert!(receipt.review_and_comment_absent());
    assert!(receipt.snapshot().is_none());
    assert_eq!(sql_state(&s.fixture), sql);
    unchanged_git_and_exact_script(&s);
}

#[test]
fn pending_reviews_v3_provider_nullable_last_parent_surviving_catalogue_or_node_or_graphql_errors_is_uncertain_not_absence(
) {
    for failure in [
        "catalogue-survivor",
        "review-node-survivor",
        "review-errors",
        "comment-node-survivor",
    ] {
        let mut s = inline_setup(|h, steps| {
            let mut select = select_steps(h, &review(h, "PENDING", "", 42));
            last_comment_responses_v3(&mut select);
            steps.extend(select);
            steps.extend(last_comment_steps_v3(h, Some(failure)));
        });
        let loaded = read(&mut s, &Default::default(), &Default::default()).unwrap();
        let selected = Arc::new(select_pending(&s, &loaded.proof));
        let sql = sql_state(&s.fixture);
        let callback = AtomicUsize::new(0);
        let error = publish_comment_v3(&s, &loaded.proof, &selected, MutationV3::Delete, &callback)
            .unwrap_err();
        assert!(
            matches!(error, ReviewPublicationFailureV2::WriteMayHaveStarted(_)),
            "{failure}"
        );
        assert_eq!(callback.load(Ordering::SeqCst), 1);
        assert_eq!(sql_state(&s.fixture), sql);
        unchanged_git_and_exact_script(&s);
    }
}

#[test]
fn pending_reviews_v3_source_malformed_or_overflow_current_coordinates_and_full_database_identity_never_downcasts(
) {
    for (field, value) in [
        ("line", json!(2147483648u64)),
        ("line", json!(0)),
        ("line", json!(-1)),
        ("line", json!("2")),
        ("startLine", json!(1.5)),
        ("fullDatabaseId", json!("18446744073709551615")),
        ("fullDatabaseId", json!("0910")),
    ] {
        let mut s = inline_setup(|h, steps| {
            steps.extend(select_steps(
                h,
                &review(h, "PENDING", "pending summary", 42),
            ));
            let mut raw = graph_comment_v3(h);
            raw[field] = value;
            steps.extend(source_steps_v3(h, graph_review_v3(h), Some(raw)));
        });
        let loaded = read(&mut s, &Default::default(), &Default::default()).unwrap();
        let selected = Arc::new(select_pending(&s, &loaded.proof));
        let sql = sql_state(&s.fixture);
        let (_, member) = original_v3(&s, &loaded.proof, selected, false).unwrap();
        assert!(member.is_err(), "{field}");
        assert_eq!(sql_state(&s.fixture), sql);
        unchanged_git_and_exact_script(&s);
    }
}

#[test]
fn pending_reviews_v3_actual_escaped_draft_bound_is_counted_before_reference_allocation_and_never_truncated(
) {
    use images::reviews_v2::operations_v3::draft_v3;
    let safe = "界".repeat(21_845);
    assert_eq!(safe.len(), 65_535);
    assert!(draft_v3::encode(&safe, None).is_ok());
    assert!(draft_v3::preflight_publication_envelope(
        &safe,
        "actual-original-node",
        &"a".repeat(40),
        &"b".repeat(64)
    )
    .is_ok());
    // Raw64KiB can expand to>128KiB under JSON escaping; neither body nor
    // publication/reference metadata can be silently shortened to fit.
    let escaped = "\u{0001}".repeat(32_768);
    assert_eq!(escaped.len(), 32_768);
    assert!(draft_v3::encode(&escaped, None).is_err());
    assert!(draft_v3::preflight_publication_envelope(
        &escaped,
        "actual-original-node",
        &"a".repeat(40),
        &"b".repeat(64)
    )
    .is_err());
    assert!(
        draft_v3::decode(r#"{"version":3,"body":"kept","publication":null,"extra":true}"#).is_err()
    );
    assert_eq!(
        draft_v3::decode(&draft_v3::encode("original exact body", None).unwrap()).unwrap(),
        ("original exact body".into(), None)
    );
}

const REVIEWER_QUERY_V3:&str="query($id:ID!){node(id:$id){__typename ... on PullRequestReview{id fullDatabaseId state submittedAt commit{oid} author{__typename login avatarUrl ... on User{id databaseId name} ... on Bot{id databaseId}} pullRequest{id fullDatabaseId number headRefOid baseRefOid repository{id databaseId nameWithOwner}} repository{id databaseId nameWithOwner}}}}";
fn reviewer_row_v3(h: &history::GitImagesV1, bot: bool) -> Value {
    // Actual PR author remains UID51. Select a DISTINCT genuine reviewer;
    // numeric self-re-request correctly remains a production refusal.
    let mut row = review(h, "APPROVED", "Actual selected submitted review", 61);
    row["user"]["type"] = json!(if bot { "Bot" } else { "User" });
    row
}
fn reviewer_node_v3(h: &history::GitImagesV1, bot: bool) -> Value {
    let parent = graph_parent_v3(h);
    json!({"__typename":"PullRequestReview","id":"ACTUAL_REVIEW_810","fullDatabaseId":"810","state":"APPROVED",
        "submittedAt":"2026-10-15T00:00:00Z","commit":{"oid":h.h},"author":{"__typename":if bot{"Bot"}else{"User"},
            "id":if bot{"ACTUAL_BOT_61"}else{"ACTUAL_USER_61"},"databaseId":61,"login":"same-login-is-not-authority","avatarUrl":null,"name":null},
        "pullRequest":parent,"repository":parent["repository"]})
}
fn requested_v3(bot: bool, selected: bool) -> Value {
    let mut users = vec![json!({"id":78,"node_id":"UNRELATED_ORIGINAL_USER_78","type":"User"})];
    if selected {
        users.push(json!({"id":61,"node_id":if bot{"ACTUAL_BOT_61"}else{"ACTUAL_USER_61"},"type":if bot{"Bot"}else{"User"}}));
    }
    json!({"users":users,"teams":[]})
}
fn rerequest_observation_v3(h: &history::GitImagesV1, bot: bool, requested: bool) -> Vec<GhStep> {
    let mut steps = snapshot_steps(h, &reviewer_row_v3(h, bot));
    steps.push(graph_step_v3(
        REVIEWER_QUERY_V3,
        "ACTUAL_REVIEW_810",
        reviewer_node_v3(h, bot),
    ));
    steps.push(get(
        &format!("repos/{REPO}/pulls/7/requested_reviewers"),
        requested_v3(bot, requested),
    ));
    steps
}
fn rerequest_steps_v3(h: &history::GitImagesV1, bot: bool, already: bool) -> Vec<GhStep> {
    let mut steps = opening_steps(h);
    steps.extend(rerequest_observation_v3(h, bot, already));
    if already {
        steps.extend(closing_steps(h));
        return steps;
    }
    steps.extend(guard_steps(h));
    steps.extend(rerequest_observation_v3(h, bot, false));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), current_parent(h)));
    steps.extend(closing_steps(h));
    let mut input = json!({"pullRequestId":current_parent(h)["node_id"]});
    input[if bot { "botIds" } else { "userIds" }] = json!([if bot {
        "ACTUAL_BOT_61"
    } else {
        "ACTUAL_USER_61"
    }]);
    steps.push(write("graphql","POST",Some(json!({"query":"mutation($input:RerequestReviewsInput!){rerequestReviews(input:$input){pullRequest{id fullDatabaseId number headRefOid baseRefOid repository{id databaseId nameWithOwner}}}}","variables":{"input":input}})),
        json!({"data":{"rerequestReviews":{"pullRequest":graph_parent_v3(h)}}})));
    steps.push(get(
        &format!("repos/{REPO}/pulls/7/requested_reviewers"),
        requested_v3(bot, true),
    ));
    steps
}
fn publish_rerequest_v3(
    s: &Setup,
    proof: &Arc<ClosedPrFileImagesV1>,
    selected: &Arc<crate::providers::gh::ReviewSnapshotV2>,
    callback: &AtomicUsize,
) -> Result<crate::providers::gh::RerequestReceiptV3, ReviewPublicationFailureV2> {
    let provider = provider(&s.fixture);
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let path = s.fixture.root.join("cache.sqlite3");
    let mut session = provider
        .begin_repository_catalogue_access(
            proof.source.catalogue_source().unwrap().source(),
            &Default::default(),
            &cancel,
        )
        .map_err(ReviewPublicationFailureV2::NotStarted)?;
    let mut captures = session
        .reserve_postwrite_capture_v2(&[1024 * 1024; 2], 2, &cancel)
        .map_err(ReviewPublicationFailureV2::NotStarted)?;
    let window = session
        .reserve_measured_external_reads(24, 1024 * 1024, 8 * 1024 * 1024, 2, &cancel)
        .map_err(ReviewPublicationFailureV2::NotStarted)?;
    let measured = provider
        .clone()
        .with_measured_read_window(window)
        .map_err(ReviewPublicationFailureV2::NotStarted)?;
    session
        .verify(proof.navigation.original(), &cancel)
        .map_err(ReviewPublicationFailureV2::NotStarted)?;
    let parent = measured
        .pr_inline_original_parent_v1(
            session
                .observation_v1(proof.navigation.original().id)
                .unwrap(),
            s.original_files.observed(),
            &Default::default(),
            &cancel,
        )
        .map_err(ReviewPublicationFailureV2::NotStarted)?;
    let identity = session.identity().clone();
    let local = ResourceLocalGuard::capture(
        &path,
        proof.source.catalogue_source().unwrap(),
        &identity,
        parent.summary(),
    )
    .map_err(ReviewPublicationFailureV2::NotStarted)?;
    let source = measured
        .rerequest_review_author_original_v3(
            &identity,
            &parent,
            selected,
            &Default::default(),
            &cancel,
            deadline,
        )
        .map_err(ReviewPublicationFailureV2::NotStarted)?;
    if source.already_requested() {
        let result = provider.rerequest_review_guarded_v3(
            &measured,
            &mut captures,
            &identity,
            &parent,
            &source,
            &source,
            &Default::default(),
            &cancel,
            deadline,
            || {
                callback.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        );
        measured
            .pr_inline_original_parent_v1(
                session
                    .observation_v1(proof.navigation.original().id)
                    .unwrap(),
                s.original_files.observed(),
                &Default::default(),
                &cancel,
            )
            .map_err(ReviewPublicationFailureV2::NotStarted)?;
        session
            .verify(proof.navigation.original(), &cancel)
            .map_err(ReviewPublicationFailureV2::NotStarted)?;
        session
            .close_with_guard(&cancel, || {
                local.validate(&path, proof.source.catalogue_source().unwrap())
            })
            .map_err(ReviewPublicationFailureV2::NotStarted)?;
        return result;
    }
    provider.rerequest_review_guarded_v3(
        &measured,
        &mut captures,
        &identity,
        &parent,
        &source,
        &source,
        &Default::default(),
        &cancel,
        deadline,
        || {
            callback.fetch_add(1, Ordering::SeqCst);
            measured.pr_inline_original_parent_v1(
                session
                    .observation_v1(proof.navigation.original().id)
                    .unwrap(),
                s.original_files.observed(),
                &Default::default(),
                &cancel,
            )?;
            session.verify(proof.navigation.original(), &cancel)?;
            session.close_with_guard(&cancel, || {
                local.validate(&path, proof.source.catalogue_source().unwrap())
            })?;
            Ok(())
        },
    )
}

#[test]
fn pending_reviews_v3_provider_actual_user_or_bot_rerequest_preserves_original_membership_and_never_claims_notification_delivery(
) {
    for bot in [false, true] {
        let mut s = inline_setup(|h, steps| {
            steps.extend(select_steps(h, &reviewer_row_v3(h, bot)));
            steps.extend(rerequest_steps_v3(h, bot, false));
        });
        let loaded = read(&mut s, &Default::default(), &Default::default()).unwrap();
        let selected = Arc::new(select_pending(&s, &loaded.proof));
        let sql = sql_state(&s.fixture);
        let callback = AtomicUsize::new(0);
        let receipt = publish_rerequest_v3(&s, &loaded.proof, &selected, &callback).unwrap();
        assert_eq!(callback.load(Ordering::SeqCst), 1);
        assert_eq!(receipt.target().id(), 61);
        assert_eq!(
            receipt.target().kind(),
            if bot {
                crate::providers::gh::ReviewRequestActorKindV3::Bot
            } else {
                crate::providers::gh::ReviewRequestActorKindV3::User
            }
        );
        assert_eq!(
            receipt.notification_status(),
            "Notification delivery unknown"
        );
        assert_eq!(sql_state(&s.fixture), sql);
        unchanged_git_and_exact_script(&s);
    }
}

#[test]
fn pending_reviews_v3_provider_already_requested_actual_member_refuses_without_blind_toggle_or_callback(
) {
    let mut s = inline_setup(|h, steps| {
        steps.extend(select_steps(h, &reviewer_row_v3(h, false)));
        steps.extend(rerequest_steps_v3(h, false, true));
    });
    let loaded = read(&mut s, &Default::default(), &Default::default()).unwrap();
    let selected = Arc::new(select_pending(&s, &loaded.proof));
    let sql = sql_state(&s.fixture);
    let callback = AtomicUsize::new(0);
    assert!(matches!(
        publish_rerequest_v3(&s, &loaded.proof, &selected, &callback),
        Err(ReviewPublicationFailureV2::NotStarted(_))
    ));
    assert_eq!(callback.load(Ordering::SeqCst), 0);
    assert_eq!(sql_state(&s.fixture), sql);
    unchanged_git_and_exact_script(&s);
}
