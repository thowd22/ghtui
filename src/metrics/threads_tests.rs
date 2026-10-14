//! Staged meaningful contracts; no result claim until this module is included and run.
use super::*;
use crate::{
    identity::ActorKind,
    providers::gh::{Actor, HostIdentity},
};

fn scope() -> MetricsScope {
    MetricsScope::new(
        &HostIdentity {
            host: "github.invalid".into(),
            login: "octocat".into(),
            account_id: Some(42),
            gh_version: "fixture".into(),
        },
        &["example/a".into(), "example/b".into()],
    )
    .unwrap()
}
fn parent(number: u64) -> ThreadParent {
    ThreadParent {
        target: PrTarget {
            repository: "example/a".into(),
            number,
            repository_id: Some(9),
            database_id: Some(100 + number),
            node_id: Some(format!("fixture-pr-node-{number}")),
            source_record_id: None,
        },
        head_sha: Some("a".repeat(40)),
    }
}
fn coverage(empty: bool) -> EvidenceCoverage {
    EvidenceCoverage {
        state: CoverageState::Complete,
        complete_empty: empty,
        fetched_at: Some(1000),
    }
}
fn participant(
    node: &str,
    role: ThreadActorRole,
    login: &str,
    actor_type: &str,
) -> ThreadParticipant {
    ThreadParticipant {
        role,
        comment_node_id: Some(node.into()),
        actor: MetricActor {
            account_id: None,
            actor: Actor {
                login: login.into(),
                actor_type: actor_type.into(),
                ..Actor::default()
            },
        },
    }
}
fn thread(id: &str, resolved: Option<bool>, outdated: Option<bool>) -> ThreadObservation {
    ThreadObservation {
        target: ThreadTarget {
            pr: parent(1).target,
            thread_node_id: id.into(),
            current_head_sha: "a".repeat(40),
            original_anchor: Some(ThreadOriginalAnchor {
                comment_node_id: format!("{id}-root-comment"),
                line: Some(12),
                start_line: Some(10),
                commit_sha: Some("b".repeat(40)),
                observed_path: Some("renamed file.rs".into()),
                observed_side: Some(ThreadSide::Right),
                verified_anchor: Some(ThreadAnchor {
                    path: "old file.rs".into(),
                    side: ThreadSide::Left,
                    line: Some(12),
                    start_line: Some(10),
                    commit_sha: Some("b".repeat(40)),
                }),
            }),
            current_anchor: Some(ThreadAnchor {
                path: "renamed file.rs".into(),
                side: ThreadSide::Right,
                line: Some(31),
                start_line: Some(29),
                commit_sha: Some("a".repeat(40)),
            }),
        },
        resolved,
        outdated,
        participants: vec![
            participant(
                &format!("{id}-root-comment"),
                ThreadActorRole::OriginalCommentAuthor,
                "robot[bot]",
                "Bot",
            ),
            participant(
                &format!("{id}-reply-comment"),
                ThreadActorRole::ReplyAuthor,
                "ghost",
                "Deleted",
            ),
        ],
    }
}
fn collection(threads: Vec<ThreadObservation>) -> ThreadCollection {
    ThreadCollection {
        scope: Scope::github("github.invalid", Some(42)).unwrap(),
        parent: parent(1),
        state_coverage: coverage(threads.is_empty()),
        provenance_coverage: coverage(threads.is_empty()),
        threads,
    }
}
fn compute(
    parents: &[ThreadParent],
    collections: &[ThreadCollection],
) -> Result<ThreadMetricCard, String> {
    unresolved_outdated_threads(
        &scope(),
        parents,
        PrCohortKind::SelectedRepositoryInventory,
        &coverage(parents.is_empty()),
        collections,
        1000,
    )
}

#[test]
fn metrics_threads_count_actual_unresolved_outdated_state_not_age_or_resolution_only() {
    let input = collection(vec![
        thread("unresolved-outdated", Some(false), Some(true)),
        thread("unresolved-current", Some(false), Some(false)),
        thread("resolved-outdated", Some(true), Some(true)),
        thread("resolved-current", Some(true), Some(false)),
    ]);
    let output = compute(&[parent(1)], &[input]).unwrap();
    assert_eq!(output.value, MetricValue::Count(1));
    assert_eq!(output.unresolved_known, 2);
    assert_eq!(output.unknown_state, 0);
    assert_eq!(
        output
            .contributors
            .iter()
            .map(|row| row.class)
            .collect::<Vec<_>>(),
        vec![
            ThreadClass::ResolvedCurrent,
            ThreadClass::ResolvedOutdated,
            ThreadClass::UnresolvedCurrent,
            ThreadClass::UnresolvedOutdated
        ]
    );
    assert!(matches!(output.coverage.state, CoverageState::Complete));
    assert!(output
        .formula
        .contains("isResolved=false AND isOutdated=true"));
    assert!(output.formula.contains("not time-based"));
}

#[test]
fn metrics_threads_preserve_original_rename_range_commit_and_actual_actor_roles() {
    let mut observation = thread("genuine-thread", Some(false), Some(true));
    observation.participants.push(participant(
        "unavailable-actor-comment",
        ThreadActorRole::ReplyAuthor,
        "",
        "Unknown",
    ));
    observation.target.current_head_sha = "c".repeat(64);
    let mut input = collection(vec![observation.clone()]);
    input.parent.head_sha = Some("c".repeat(64));
    let output = compute(&[input.parent.clone()], &[input]).unwrap();
    let row = &output.contributors[0];
    assert_eq!(row.target, observation.target);
    let original_source = row.target.original_anchor.as_ref().unwrap();
    assert_eq!(
        original_source.observed_path.as_deref(),
        Some("renamed file.rs")
    );
    assert_eq!(original_source.observed_side, Some(ThreadSide::Right));
    let original = original_source.verified_anchor.as_ref().unwrap();
    assert_eq!(
        (
            &*original.path,
            original.side,
            original.start_line,
            original.line
        ),
        ("old file.rs", ThreadSide::Left, Some(10), Some(12))
    );
    assert_eq!(
        original.commit_sha.as_deref(),
        Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
    );
    assert_eq!(row.target.current_head_sha.len(), 64);
    assert_eq!(
        row.participants[0].role,
        ThreadActorRole::OriginalCommentAuthor
    );
    assert_eq!(
        row.participants[0].comment_node_id.as_deref(),
        Some("genuine-thread-root-comment")
    );
    assert_eq!(
        row.participants[0].actor.identity(&scope()).kind,
        ActorKind::Bot
    );
    assert_eq!(row.participants[1].role, ThreadActorRole::ReplyAuthor);
    assert_eq!(
        row.participants[1].actor.identity(&scope()).kind,
        ActorKind::Deleted
    );
    assert_eq!(
        row.participants[2].actor.identity(&scope()).kind,
        ActorKind::Unknown
    );
}

#[test]
fn metrics_threads_missing_original_anchor_never_uses_current_coordinates_or_head() {
    let mut candidate = thread("unverified-old-path", Some(false), Some(true));
    candidate
        .target
        .original_anchor
        .as_mut()
        .unwrap()
        .verified_anchor = None;
    let output = compute(&[parent(1)], &[collection(vec![candidate])]).unwrap();
    let original = output.contributors[0]
        .target
        .original_anchor
        .as_ref()
        .unwrap();
    assert_eq!(original.line, Some(12));
    assert_eq!(
        original.commit_sha.as_deref(),
        Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
    );
    assert_eq!(original.observed_path.as_deref(), Some("renamed file.rs"));
    assert!(original.verified_anchor.is_none());
    assert!(
        !original.complete(),
        "Observed path candidate cannot authorize an original-tree jump"
    );
    assert!(matches!(
        output.provenance_coverage.state,
        CoverageState::Partial(_)
    ));
    let mut observation = thread("outdated", Some(false), Some(true));
    observation.target.original_anchor = None;
    let output = compute(&[parent(1)], &[collection(vec![observation])]).unwrap();
    assert_eq!(output.value, MetricValue::Count(1));
    assert!(matches!(output.coverage.state, CoverageState::Complete));
    assert!(matches!(
        output.provenance_coverage.state,
        CoverageState::Partial(_)
    ));
    assert!(output.contributors[0].target.original_anchor.is_none());
    assert_eq!(
        output.contributors[0]
            .target
            .current_anchor
            .as_ref()
            .unwrap()
            .line,
        Some(31)
    );
    let mut observation = thread("null-original-line", Some(false), Some(true));
    observation.target.original_anchor.as_mut().unwrap().line = None;
    observation
        .target
        .original_anchor
        .as_mut()
        .unwrap()
        .start_line = None;
    observation
        .target
        .original_anchor
        .as_mut()
        .unwrap()
        .verified_anchor = None;
    let output = compute(&[parent(1)], &[collection(vec![observation])]).unwrap();
    assert!(output.contributors[0]
        .target
        .original_anchor
        .as_ref()
        .unwrap()
        .line
        .is_none());
    assert!(matches!(
        output.provenance_coverage.state,
        CoverageState::Partial(_)
    ));
}

#[test]
fn metrics_threads_outer_state_completeness_is_independent_from_capped_inner_actors() {
    let mut input = collection(vec![thread("stale", Some(false), Some(true))]);
    input
        .provenance_coverage
        .partial("Only first nested comment page admitted");
    let output = compute(&[parent(1)], &[input]).unwrap();
    assert_eq!(output.value, MetricValue::Count(1));
    assert!(matches!(output.coverage.state, CoverageState::Complete));
    assert!(matches!(
        output.provenance_coverage.state,
        CoverageState::Partial(_)
    ));
    assert_eq!(output.contributors[0].participants.len(), 2);
    assert!(output.contributors[0].target.validate(&scope()).is_ok());
}

#[test]
fn metrics_threads_missing_state_and_unloaded_parents_keep_partial_known_ids_not_false_or_zero() {
    let input = collection(vec![
        thread("known", Some(false), Some(true)),
        thread("missing-outdated", Some(false), None),
        thread("missing-resolution", None, Some(true)),
    ]);
    let output = compute(&[parent(1), parent(2)], &[input]).unwrap();
    assert_eq!(output.value, MetricValue::Count(1));
    assert_eq!(output.unresolved_known, 2);
    assert_eq!(output.unknown_state, 2);
    assert_eq!(output.missing_parents, vec![parent(2).target]);
    assert!(matches!(output.coverage.state, CoverageState::Partial(_)));
    assert_eq!(
        output
            .contributors
            .iter()
            .filter(|row| row.class == ThreadClass::Unknown)
            .count(),
        2
    );
    let no_known_stale = compute(
        &[parent(1)],
        &[collection(vec![thread("unknown", Some(false), None)])],
    )
    .unwrap();
    assert_eq!(no_known_stale.value, MetricValue::Unavailable);
    assert_eq!(no_known_stale.unknown_state, 1);
}

#[test]
fn metrics_threads_zero_requires_complete_state_source_and_explicit_empty_parent_proof() {
    assert_eq!(compute(&[], &[]).unwrap().value, MetricValue::Count(0));
    assert_eq!(
        compute(&[parent(1)], &[collection(vec![])]).unwrap().value,
        MetricValue::Count(0)
    );
    assert_eq!(
        compute(
            &[parent(1)],
            &[collection(vec![thread("resolved", Some(true), Some(true))])]
        )
        .unwrap()
        .value,
        MetricValue::Count(0)
    );
    let mut missing = collection(vec![]);
    missing.state_coverage.complete_empty = false;
    assert_eq!(
        compute(&[parent(1)], &[missing]).unwrap().value,
        MetricValue::Unavailable
    );
    assert_eq!(
        compute(&[parent(1)], &[]).unwrap().value,
        MetricValue::Unavailable
    );
    let missing_cohort = unresolved_outdated_threads(
        &scope(),
        &[],
        PrCohortKind::PersonalQueueUnion,
        &EvidenceCoverage::unavailable("No queue loaded"),
        &[],
        1000,
    )
    .unwrap();
    assert_eq!(missing_cohort.value, MetricValue::Unavailable);
    assert!(missing_cohort.cohort.contains("not repository-wide"));
}

#[test]
fn metrics_threads_canonical_scope_parent_head_and_conflicting_node_cannot_be_aliased() {
    let base = collection(vec![thread("genuine-node", Some(false), Some(true))]);
    let mut wrong = base.clone();
    wrong.scope = Scope::github("github.invalid", Some(43)).unwrap();
    assert!(compute(&[parent(1)], &[wrong]).is_err());
    let mut wrong = base.clone();
    wrong.threads[0].target.pr.database_id = Some(999);
    assert!(compute(&[parent(1)], &[wrong]).is_err());
    let mut wrong = base.clone();
    wrong.threads[0].target.current_head_sha = "a".repeat(8);
    assert!(compute(&[parent(1)], &[wrong]).is_err());
    let mut wrong = base.clone();
    wrong.parent.target.node_id = None;
    assert!(compute(&[wrong.parent.clone()], &[wrong]).is_err());
    let mut duplicate = base.clone();
    duplicate.threads.push(duplicate.threads[0].clone());
    assert_eq!(
        compute(&[parent(1)], &[duplicate.clone()]).unwrap().value,
        MetricValue::Count(1)
    );
    duplicate.threads[1].resolved = Some(true);
    assert!(compute(&[parent(1)], &[duplicate]).is_err());
    let mut reused = base.clone();
    let mut second = thread("other-thread", Some(false), Some(true));
    second.participants[0].comment_node_id =
        reused.threads[0].participants[0].comment_node_id.clone();
    reused.threads.push(second);
    assert!(compute(&[parent(1)], &[reused]).is_err());
}

#[test]
fn metrics_threads_bound_raw_records_and_refuse_bad_original_ranges_or_foreign_actor_nodes() {
    let mut input = collection(vec![thread("node", Some(false), Some(true))]);
    input.threads[0]
        .target
        .original_anchor
        .as_mut()
        .unwrap()
        .start_line = Some(13);
    assert!(compute(&[parent(1)], &[input]).is_err());
    let mut input = collection(vec![thread("node", Some(false), Some(true))]);
    input.threads[0]
        .target
        .original_anchor
        .as_mut()
        .unwrap()
        .observed_path = Some("../escape.rs".into());
    assert!(compute(&[parent(1)], &[input]).is_err());
    let mut input = collection(vec![thread("node", Some(false), Some(true))]);
    input.threads[0].participants[0].comment_node_id = None;
    assert!(compute(&[parent(1)], &[input]).is_err());
    let mut input = collection(vec![thread("node", Some(false), Some(true))]);
    input.state_coverage.fetched_at = Some(1001);
    assert!(compute(&[parent(1)], &[input]).is_err());
    let input = collection(vec![
        thread("same-source-node", Some(false), Some(true));
        MAX_THREADS + 1
    ]);
    assert!(
        compute(&[parent(1)], &[input]).is_err(),
        "Raw duplicate inputs still consume the bounded work budget"
    );
}
