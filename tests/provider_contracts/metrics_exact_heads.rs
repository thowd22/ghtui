//! SOURCE-ONLY genuine full-SHA correction contracts. Legacy validate_sha unchanged.
use super::*;

fn current() -> HostIdentity {
    HostIdentity {
        account_id: Some(42),
        ..identity()
    }
}
fn repo() -> Value {
    json!({"id":99,"node_id":"R_99","full_name":"example/repo"})
}
fn parent(head: Value) -> Value {
    json!({"id":101,"node_id":"PR_1","number":1,"title":"Original","state":"open","draft":false,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-12T00:00:00Z","merged_at":null,"closed_at":null,"merged":false,"head":{"sha":head},"base":{"repo":repo()},"user":{"id":51,"login":"alice"},"requested_reviewers":[],"requested_teams":[]})
}
fn initial(raw: Value) -> Vec<GhStep> {
    vec![step(api_args("repos/example/repo"),repo()),step(api_args("repos/example/repo/pulls?state=all&sort=created&direction=desc&per_page=100&page=1"),json!([raw])),step(api_args("repos/example/repo"),repo())]
}
fn load(provider: &GithubProvider) -> ghtui::providers::gh::RepositoryPrInventory {
    provider
        .repository_pr_inventory(
            &current(),
            "example/repo",
            None,
            &Default::default(),
            &CancellationToken::default(),
        )
        .unwrap()
}
fn assert_read_count(fixture: &Fixture, count: usize) {
    let records = journal(fixture);
    assert_eq!(records.len(), count);
    assert!(records
        .iter()
        .all(|row| row["matched"] == true && row["args"][5] == "GET" && row["stdin_hex"] == ""));
}

#[test]
fn metrics_exact_heads_provider_short_prefixes_and_nonhex_source_heads_remain_unknown_partial() {
    for head in [
        "a".repeat(7),
        "a".repeat(33),
        "a".repeat(39),
        "a".repeat(41),
        "a".repeat(63),
        "a".repeat(65),
        "z".repeat(40),
    ] {
        let fixture = Fixture::new();
        fixture.scripted_gh(&initial(parent(json!(head))));
        let snapshot = load(&provider(&fixture));
        assert!(snapshot.coverage.enumeration_complete);
        assert!(!snapshot.coverage.metric_evidence_complete);
        assert!(snapshot.prs[0].head_sha.is_none());
        assert!(snapshot.prs[0]
            .reasons
            .iter()
            .any(|reason| reason.contains("Exact PR head")));
        assert_eq!(snapshot.prs[0].id, 101);
        assert_eq!(snapshot.prs[0].node_id, "PR_1");
        assert_read_count(&fixture, 3);
    }
}
#[test]
fn metrics_exact_heads_provider_actual_sha1_and_sha256_are_retained_without_legacy_prefix_guess() {
    for head in ["a".repeat(40), "B".repeat(64)] {
        let fixture = Fixture::new();
        fixture.scripted_gh(&initial(parent(json!(head))));
        let snapshot = load(&provider(&fixture));
        assert!(
            snapshot.coverage.enumeration_complete && snapshot.coverage.metric_evidence_complete
        );
        assert_eq!(snapshot.prs[0].head_sha.as_deref(), Some(head.as_str()));
        assert_read_count(&fixture, 3);
    }
}
#[test]
fn metrics_exact_heads_provider_review_commit_prefix_never_becomes_current_head_or_complete_evidence(
) {
    for sha in [
        "a".repeat(7),
        "b".repeat(33),
        "z".repeat(40),
        "b".repeat(64),
    ] {
        let fixture = Fixture::new();
        let raw = parent(json!("a".repeat(40)));
        let mut reads = initial(raw.clone());
        reads.extend([step(api_args("repos/example/repo"),repo()),step(api_args("repos/example/repo/pulls/1"),raw.clone()),step(api_args("repos/example/repo/pulls/1/reviews?per_page=100&page=1"),json!([{"id":201,"node_id":"REV_201","state":"APPROVED","submitted_at":"2026-10-02T00:00:00Z","user":{"id":52,"login":"reviewer"},"commit_id":sha}])),step(api_args("repos/example/repo/issues/1/timeline?per_page=100&page=1"),json!([])),step(api_args("repos/example/repo"),repo()),step(api_args("repos/example/repo/pulls/1"),raw)]);
        fixture.scripted_gh(&reads);
        let provider = provider(&fixture);
        let original = load(&provider).prs.remove(0);
        let snapshot = provider
            .pr_metric_events(
                &current(),
                &original,
                &Default::default(),
                &CancellationToken::default(),
            )
            .unwrap();
        assert!(snapshot.review_coverage.enumeration_complete);
        let valid = sha.len() == 64;
        assert_eq!(snapshot.review_coverage.metric_evidence_complete, valid);
        assert_eq!(
            snapshot.reviews[0].commit_sha.as_deref(),
            if valid { Some(sha.as_str()) } else { None }
        );
        assert_eq!(
            snapshot.reviews[0].submitted_at.as_ref().unwrap().raw(),
            "2026-10-02T00:00:00Z"
        );
        assert_eq!(
            snapshot.parent.head_sha.as_deref(),
            Some("a".repeat(40).as_str())
        );
        assert_read_count(&fixture, 9);
    }
}
