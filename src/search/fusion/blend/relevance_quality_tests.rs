//! Undeclared fixed-quality judgments over real canonical cache projections and
//! prevalidated ordinal lists. Registration and nonzero actual gates are pending.
//! These are relevance/explanation contracts, not timing or live-provider proof.
use super::*;

#[test]
fn collaboration_index_search_relevance_quality_canonical_answer_agreement_duplicates_and_explanation(
) {
    let context = context();
    let mut cache = Cache::memory().unwrap();
    let answer = snapshot(
        DiscussionKind::PullRequestBody,
        "needle AuthBoundary canonical design answer, not an old indexed excerpt",
    );
    let supporting_comment = snapshot(
        DiscussionKind::ConversationComment,
        "needle supporting AuthBoundary reviewer context",
    );
    publish(&mut cache, &answer);
    publish(&mut cache, &supporting_comment);
    let targets = [answer.target, supporting_comment.target];
    let documents = cached(&cache, &targets);
    let answer = documents
        .iter()
        .find(|document| document.key.kind == ResultKind::PullRequest)
        .unwrap();
    let supporting_comment = documents
        .iter()
        .find(|document| document.key.kind == ResultKind::Comment)
        .unwrap();
    // Fixed product judgment: an exact canonical answer independently supported
    // at local rank3 and provider rank2 beats single-source distractors. The
    // short supporting comment remains a distinct useful result, not its parent.
    let local = local_page(
        &context,
        vec![
            hit(supporting_comment.clone(), 9_000_000.0),
            hit(local_file("needle-body-only-distractor.rs"), 8_000_000.0),
            hit(answer.clone(), 0.00001),
        ],
    );
    let mut candidates = vec![
        remote_number(
            remote_parent("provider-first-distractor", node_identity("PR_other")),
            8,
        ),
        remote_parent("provider-answer", node_identity("PR_parent")),
    ];
    candidates.extend((0..98).map(|number| {
        remote_parent(
            &format!("duplicate-answer-{number}"),
            node_identity("PR_parent"),
        )
    }));
    assert_eq!(candidates.len(), 100);
    let remote = remote_list(&context, candidates);
    let visibility = current(&cache, &targets, &context);
    let output = run(
        &cache,
        &context,
        &local,
        std::slice::from_ref(&remote),
        &visibility,
    );
    let best = &output.page.hits[0];
    assert_eq!(
        best.document.key, answer.key,
        "The independently judged canonical answer must rank first"
    );
    assert_eq!(best.document.body, answer.body);
    assert_eq!(best.snippet, answer.body);
    assert!(!best.snippet.contains("stale remote body"));
    assert_eq!(
        output
            .page
            .hits
            .iter()
            .filter(|hit| hit.document.key == answer.key)
            .count(),
        1,
        "One hundred provider rows must not become duplicate answers or votes"
    );
    assert!(output
        .page
        .hits
        .iter()
        .any(|hit| hit.document.key == supporting_comment.key));
    let key = answer.key.stable_id();
    assert_eq!(
        output.contributions[&key],
        [
            (RankingSource::LocalLexical, 3),
            (RankingSource::GithubPullRequests, 2)
        ]
    );
    assert_eq!(
        best.matched_fields.last().unwrap(),
        "RRF k=60: Local lexical #3 + GitHub PRs #2"
    );
    let expected = 1.0 / 63.0 + 1.0 / 62.0;
    assert!(
        (f64::from(best.score) - expected).abs() < 1e-7,
        "Displayed fused score must agree with the exact explained ordinal contributions"
    );
    assert!(matches!(
        best.destination(),
        Destination::PullRequest {
            number: 7,
            comment_id: None,
            thread_id: None,
            ..
        }
    ));
    assert_eq!(
        visibility
            .validate_document(&cache, &best.document)
            .unwrap(),
        Some(targets[0].clone())
    );
    visibility.validate(&cache).unwrap();
    let mut inverted = remote.clone();
    for candidate in &mut inverted.candidates {
        candidate.hit.score = 0.000001;
    }
    let again = run(&cache, &context, &local, &[inverted], &visibility);
    assert_eq!(
        again
            .page
            .hits
            .iter()
            .map(|hit| (&hit.document.key, hit.score))
            .collect::<Vec<_>>(),
        output
            .page
            .hits
            .iter()
            .map(|hit| (&hit.document.key, hit.score))
            .collect::<Vec<_>>(),
        "Provider raw engine scores never alter the fixed rank-fusion judgment"
    );
}

#[test]
fn collaboration_index_search_relevance_quality_source_saturation_cannot_outvote_local_top_answer()
{
    let local = RankList {
        source: RankingSource::LocalLexical,
        entries: vec![RankEntry {
            alias: "judged-local-primary-answer".into(),
            representative: "exact-local-answer".into(),
            presentation: Presentation::AuthorizedCurrentDocument,
        }],
    };
    let remote = RankList {
        source: RankingSource::GithubPullRequests,
        entries: (0..1000)
            .map(|rank| RankEntry {
                alias: format!("provider-distractor-{rank:04}"),
                representative: format!("provider-fragment-{rank:04}"),
                presentation: Presentation::RemoteFragment,
            })
            .collect(),
    };
    let baseline = fuse(&[local.clone(), remote.clone()], 2).unwrap();
    assert_eq!(baseline.len(), 2);
    let answer = baseline
        .iter()
        .find(|entry| entry.alias == "judged-local-primary-answer")
        .unwrap();
    assert_eq!(answer.representative, "exact-local-answer");
    assert_eq!(answer.score, 1.0 / 61.0);
    assert_eq!(answer.contributions, [(RankingSource::LocalLexical, 1)]);
    assert_eq!(
        baseline,
        fuse(&[remote, local.clone()], 2).unwrap(),
        "Source ordering is not a hidden priority or additional vote"
    );
    let duplicates = RankList {
        source: RankingSource::GithubPullRequests,
        entries: (0..1000)
            .map(|rank| RankEntry {
                alias: "one-provider-distractor".into(),
                representative: format!("same-answer-presentation-{rank:04}"),
                presentation: Presentation::RemoteFragment,
            })
            .collect(),
    };
    let duplicated = fuse(&[local, duplicates], 2).unwrap();
    assert_eq!(duplicated.len(), 2);
    assert!(duplicated
        .iter()
        .any(|entry| entry.alias == "judged-local-primary-answer"));
    let provider = duplicated
        .iter()
        .find(|entry| entry.alias == "one-provider-distractor")
        .unwrap();
    assert_eq!(provider.score, 1.0 / 61.0);
    assert_eq!(
        provider.contributions,
        [(RankingSource::GithubPullRequests, 1)],
        "A thousand same-source duplicate rows provide only one source-rank vote"
    );
}
