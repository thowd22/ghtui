//! Genuine initial Personal factory/Core/gh contracts. These direct worker
//! tests are NOT prequeue/native-order or postwrite-publication evidence.
use super::*;
use crate::{
    providers::gh::{ConversationDetailSnapshotV19, GithubResult},
    scoped_pr_observed_tabs_v1::resources::ResourceLocalGuard,
};

fn full_parent() -> Value {
    let mut raw = parent();
    raw["additions"] = json!(1);
    raw["deletions"] = json!(1);
    raw["created_at"] = json!("2026-10-15T01:00:00Z");
    raw["updated_at"] = json!("2026-10-15T02:00:00Z");
    raw["mergeable"] = Value::Null;
    raw["mergeable_state"] = json!("unknown");
    raw
}
fn lists() -> Vec<GhStep> {
    [
        ("issues/7/comments",json!([[{"id":901,"node_id":"IC_901","body":"Actual fresh discussion 界","user":{"id":43,"login":"other-author","type":"User"},"created_at":"2026-10-15T01:00:00Z","updated_at":"2026-10-15T02:00:00Z","html_url":format!("https://github.invalid/{REPO}/pull/7#issuecomment-901")}]])),
        ("pulls/7/commits",json!([[{"sha":"a".repeat(40),"commit":{"message":"Actual saved commit","author":{"name":"Original author","date":"2026-10-15T01:00:00Z"},"committer":{"name":"Other recorder"}},"author":null,"committer":null,"parents":[]}]])),
        ("pulls/7/files",json!([[{"filename":"src/view.rs","status":"modified","additions":1,"deletions":1,"patch":"@@ -1 +1 @@\n-old\n+new\n"}]])),
        ("pulls/7/reviews",json!([[]])),("issues/7/timeline",json!([[]])),
    ].into_iter().map(|(path,value)|{let mut step=get(&format!("repos/{REPO}/{path}?per_page=100"),value);step.args.extend(["--paginate".into(),"--slurp".into()]);step}).collect()
}
pub(crate) fn refresh_phase() -> Vec<GhStep> {
    let mut calls = principal();
    calls.push(get(&format!("repos/{REPO}"), repository()));
    calls.push(get(&format!("repos/{REPO}/pulls/7"), full_parent()));
    calls.extend(lists());
    calls.push(get(&format!("repos/{REPO}/pulls/7"), full_parent()));
    calls.push(get(&format!("repos/{REPO}"), repository()));
    calls.extend(principal());
    assert_eq!(calls.len(), 13);
    calls
}
fn read(s: &Setup, deadline: Instant, cap: usize) -> GithubResult<ConversationDetailSnapshotV19> {
    let original = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let cancel = CancellationToken::default();
    let bounded = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, cap)?;
    source::with_refresh_read_v19(
        original,
        &bounded,
        &database(&s.fixture),
        &cancel,
        deadline,
        |read| {
            let local = ResourceLocalGuard::capture_boundary_v13(
                &database(&s.fixture),
                read.boundary,
                read.identity,
                read.preparation.parent().summary(),
            )?;
            Ok((local, read.snapshot))
        },
        |(local, _), boundary| local.validate_boundary_v13(&database(&s.fixture), boundary),
    )
    .map(|closed| {
        let ((_, snapshot), identity, _seal) = closed.into_material_v19();
        assert_eq!(identity.account_id, Some(42));
        snapshot
    })
}
#[test]
fn conversation_refresh_v19_actual_personal_thirteen_child_full_summary_not_stripped_parent() {
    let s = setup(refresh_phase());
    let page = allocations(&s.app);
    let snapshot = read(&s, Instant::now() + Duration::from_secs(30), 1024 * 1024).unwrap();
    assert_eq!(snapshot.details().summary.pr.body, BODY);
    assert_eq!(
        snapshot.details().summary.pr.created_at,
        "2026-10-15T01:00:00Z"
    );
    assert_eq!(snapshot.details().summary.additions, 1);
    assert_eq!(
        snapshot.details().comments[0].body,
        "Actual fresh discussion 界"
    );
    assert_eq!(snapshot.details().comments[0].author.login, "other-author");
    assert!(snapshot.details().coverage.complete);
    assert_eq!(allocations(&s.app), page);
    exact(&s, s.steps.len());
}
#[test]
fn conversation_refresh_v19_full_emitted_digest_refuses_body_title_actor_time_count_and_merge_drift(
) {
    for (key, value) in [
        ("body", json!("Changed independent body")),
        ("title", json!("Changed actual title")),
        (
            "user",
            json!({"id":51,"node_id":"AUTHOR_51","login":"renamed-real-author","type":"User"}),
        ),
        ("updated_at", json!("2026-10-15T03:00:00Z")),
        ("additions", json!(2)),
        ("mergeable", json!(true)),
    ] {
        let mut calls = refresh_phase();
        let mut closing = full_parent();
        closing[key] = value;
        calls[9] = get(&format!("repos/{REPO}/pulls/7"), closing);
        let s = setup(calls);
        let page = allocations(&s.app);
        assert!(
            read(&s, Instant::now() + Duration::from_secs(30), 1024 * 1024).is_err(),
            "{key}"
        );
        assert_eq!(allocations(&s.app), page);
        exact(&s, opening().len() + 10);
    }
}
#[test]
fn conversation_refresh_v19_missing_nullable_counts_never_become_zero_fresh_projection() {
    for key in ["additions", "deletions", "changed_files"] {
        let mut calls = refresh_phase();
        let mut raw = full_parent();
        raw[key] = Value::Null;
        calls[3] = get(&format!("repos/{REPO}/pulls/7"), raw);
        let s = setup(calls);
        assert!(read(&s, Instant::now() + Duration::from_secs(30), 1024 * 1024).is_err());
        exact(&s, opening().len() + 4);
    }
}
#[test]
fn conversation_refresh_v19_duplicate_missing_page_and_malformed_list_refuse_without_closing_or_publication(
) {
    for value in [
        json!([]),
        json!([{"id":1}]),
        json!([[{"id":901,"node_id":"IC_901","body":"one"},{"id":901,"node_id":"IC_902","body":"two"}]]),
        json!([[{"id":901,"node_id":"IC_901"}]]),
    ] {
        let mut calls = refresh_phase();
        calls[4].stdout = serde_json::to_vec(&value).unwrap();
        let s = setup(calls);
        assert!(read(&s, Instant::now() + Duration::from_secs(30), 1024 * 1024).is_err());
        exact(&s, opening().len() + 5);
    }
}
#[test]
fn conversation_refresh_v19_expiry_zero_children_and_original_narrow_capture_cannot_widen() {
    let s = setup(Vec::new());
    assert!(read(&s, Instant::now() - Duration::from_millis(1), 1024 * 1024).is_err());
    exact(&s, opening().len());
    let mut calls = refresh_phase();
    let mut raw = full_parent();
    raw["body"] = json!("x".repeat(8192));
    calls[3] = get(&format!("repos/{REPO}/pulls/7"), raw);
    let s = setup(calls);
    assert!(read(&s, Instant::now() + Duration::from_secs(30), 4096).is_err());
    exact(&s, opening().len() + 4);
}
#[test]
fn conversation_refresh_v19_file_coverage_stays_honest_partial_without_review_or_inline_authority()
{
    let mut calls = refresh_phase();
    for index in [3, 9] {
        let mut raw = full_parent();
        raw["changed_files"] = json!(3001);
        calls[index] = get(&format!("repos/{REPO}/pulls/7"), raw);
    }
    let s = setup(calls);
    let snapshot = read(&s, Instant::now() + Duration::from_secs(30), 1024 * 1024).unwrap();
    assert!(!snapshot.details().coverage.complete);
    assert_eq!(snapshot.details().coverage.total_matches, 3001);
    assert_eq!(snapshot.details().coverage.loaded, 1);
    assert_eq!(snapshot.details().reviews.len(), 0);
    exact(&s, s.steps.len());
}
