//! Genuine native terminal/OwnFence -> NEW actual Personal Core. Neither a
//! saved row nor the old receipt is a fresh Source or read-window authority.
use super::*;
#[path = "manual_metadata_v101_tests.rs"]
mod manual_metadata_v101_tests;
use crate::{
    scoped_pr_collaboration_refresh_v19::PreparedConversationPageV19,
    scoped_pr_collaboration_source_v17::issue_links_phase_v87::{
        with_issue_link_manual_refresh_v94, IssueLinkManualPhaseV94,
    },
    storage::conversation_refresh_v19::PreparedConversationOwnFenceV19,
};

fn fresh_raw_v94() -> Value {
    let mut raw = personal::parent();
    raw["additions"] = json!(1);
    raw["deletions"] = json!(1);
    raw["created_at"] = json!("2026-10-16T03:00:00Z");
    raw["updated_at"] = json!("2026-10-16T04:00:00Z");
    raw
}
fn fresh_pair_v94() -> Value {
    let post = post_v87();
    let pr: Value = serde_json::from_slice(&post[1].stdout).unwrap();
    let issue: Value = serde_json::from_slice(&post[2].stdout).unwrap();
    json!({"data":{"pr":pr["data"]["pr"],"issue":issue["data"]["issue"]}})
}
fn fresh_steps_v94() -> Vec<GhStep> {
    let mut calls = personal::principal();
    calls.push(personal::get(
        &format!("repos/{}", personal::REPO),
        writable_repository(),
    ));
    calls.push(personal::get(
        &format!("repos/{}/pulls/7", personal::REPO),
        fresh_raw_v94(),
    ));
    calls.push(graphql(protocol::CAPABILITY_QUERY_V87, json!({}), schema()));
    calls.push(personal::get(
        &format!("repos/{}", personal::REPO),
        writable_repository(),
    ));
    calls.push(graphql(
        &protocol::pair_query(),
        json!({"pr":"PERSONAL_PR_1007","issue":"ISSUE_20009"}),
        fresh_pair_v94(),
    ));
    for endpoint in [
        "issues/7/comments",
        "pulls/7/commits",
        "pulls/7/files",
        "pulls/7/reviews",
        "issues/7/timeline",
    ] {
        let raw = if endpoint == "pulls/7/files" {
            json!([[{"filename":"src/view.rs","status":"modified","additions":1,"deletions":1,"patch":"@@ -1 +1 @@\n-old\n+new\n"}]])
        } else {
            json!([[]])
        };
        calls.push(GhStep::read(
            &[
                "api",
                &format!("repos/{}/{endpoint}?per_page=100", personal::REPO),
                "--hostname",
                "github.invalid",
                "--method",
                "GET",
                "--paginate",
                "--slurp",
            ],
            &raw.to_string(),
        ));
    }
    calls.push(graphql(
        &protocol::pair_query(),
        json!({"pr":"PERSONAL_PR_1007","issue":"ISSUE_20009"}),
        fresh_pair_v94(),
    ));
    calls.push(graphql(
        &protocol::issue_query(),
        json!({"issue":"ISSUE_20009"}),
        json!({"data":{"issue":issue(9)}}),
    ));
    calls.push(personal::get(
        &format!("repos/{}", personal::REPO),
        writable_repository(),
    ));
    calls.push(personal::get(
        &format!("repos/{}/pulls/7", personal::REPO),
        fresh_raw_v94(),
    ));
    calls.push(personal::get(
        &format!("repos/{}", personal::REPO),
        writable_repository(),
    ));
    calls.extend(personal::principal());
    assert_eq!(calls.len(), 19);
    calls
}
fn terminal_v94(
    later: Vec<GhStep>,
) -> (
    personal::Setup,
    DraftWriter,
    ClosedReceiptV87,
    Arc<crate::storage::conversation_refresh_v19::ConversationOwnFenceV19>,
) {
    let mut post = post_v87();
    post.extend(later);
    let s = publication_setup_v87(post);
    let (writer, input) = publication_v87(&s);
    let closed = execute_receipt_v87(input).unwrap_or_else(|e| panic!("{}", e.message));
    let own = PreparedConversationOwnFenceV19::prepare(
        Arc::clone(&closed.terminal),
        &closed.cancellation,
        closed.deadline,
    )
    .unwrap()
    .commit(&closed.cancellation, closed.deadline)
    .unwrap();
    assert!(own.is_personal_v20());
    (s, writer, closed, own)
}
fn fresh_v94(
    s: &personal::Setup,
    own: &Arc<crate::storage::conversation_refresh_v19::ConversationOwnFenceV19>,
    cancel: &CancellationToken,
) -> GithubResult<crate::scoped_pr_collaboration_refresh_v19::AdmittedConversationPageV19> {
    let deadline = Instant::now() + Duration::from_secs(30);
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    let attempt = own
        .begin_personal_read_attempt_v20(None, &provider, cancel, deadline)
        .map_err(|_| refused(GithubErrorKind::IdentityChanged))?;
    let closed = with_issue_link_manual_refresh_v94(
        IssueLinkManualPhaseV94 {
            own,
            attempt: Some(&attempt),
        },
        &provider,
        own.terminal().database(),
        cancel,
        deadline,
        |read| {
            assert!(Arc::ptr_eq(
                read.snapshot.parent(),
                read.preparation.parent()
            ));
            let ConversationPageSupplementV61::IssueLinksV87(context) = &read.page_supplement_v61
            else {
                panic!("Actual fresh Issue context required")
            };
            assert!(context.matches_source_v87(own.source()));
            assert!(own.terminal().validates_issue_link_context_v87(context));
            context.prepare_fixed_record_v87(&read.snapshot, own.terminal())?;
            PreparedConversationPageV19::prepare_read_v19(own, read, cancel)
        },
        |_, boundary| {
            let cache = crate::storage::Cache::open(own.terminal().database())
                .map_err(|_| refused(GithubErrorKind::IdentityChanged))?;
            attempt
                .validate_during_read(&cache, boundary)
                .map_err(|_| refused(GithubErrorKind::IdentityChanged))?;
            let journal = PrWriteJournal::open(own.terminal().database())
                .map_err(|_| refused(GithubErrorKind::IdentityChanged))?;
            own.terminal()
                .validate_current_v19(&journal)
                .map_err(|_| refused(GithubErrorKind::IdentityChanged))
        },
    )?;
    PreparedConversationPageV19::admit_read_v19(closed, cancel, deadline)
}
fn protected_sql_v94(s: &personal::Setup) -> Vec<String> {
    personal::sql(&s.fixture)
        .into_iter()
        .filter(|row| {
            row.starts_with("drafts:")
                || row.starts_with("pr_write_")
                || row.starts_with("sync_mutation_fences:")
        })
        .collect()
}
#[test]
fn pr_metadata_issue_link_v87_fresh_v94_actual_terminal_ownfence_new_personal_core19_commits_fixed_context_without_cleanup_or_resend(
) {
    let (s, writer, closed, own) = terminal_v94(fresh_steps_v94());
    let protected = protected_sql_v94(&s);
    let schema = schema_sql(&s);
    let cancel = CancellationToken::default();
    assert!(!cancel.same_instance(&closed.cancellation));
    let page = fresh_v94(&s, &own, &cancel).unwrap_or_else(|e| panic!("{}", e.message));
    drop(page);
    assert_eq!(protected_sql_v94(&s), protected);
    assert_eq!(schema_sql(&s), schema);
    protected_v87(&s, "confirmed");
    let db = rusqlite::Connection::open_with_flags(
        &closed.database,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let payload: String = db.query_row("SELECT CAST(payload AS TEXT) FROM records WHERE host='github.invalid' AND login='user-id:42' AND kind='pr-manual-issue-links-v87' AND id='PERSONAL_PR_1007'", [], |row| row.get(0)).unwrap();
    let payload: Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(payload["command"], "link_issue");
    assert_eq!(payload["requested_plan_sha256"], closed.plan.digest());
    assert_eq!(
        payload["observed"]["forward"][1]["20009"]["node"],
        "ISSUE_20009"
    );
    assert_eq!(
        payload["observed"]["reverse"][1]["1007"]["node"],
        "PERSONAL_PR_1007"
    );
    transport_and_git_protocol(&s, Some(IssueLinkCommandV87::LinkIssue));
    drop(writer);
}
#[test]
fn pr_metadata_issue_link_v87_fresh_v94_actual_final_uid_loss_never_admits_context_or_deletes_confirmed_body(
) {
    let mut steps = fresh_steps_v94();
    steps.last_mut().unwrap().stdout =
        serde_json::to_vec(&json!({"id":43,"login":"other-account"})).unwrap();
    let (s, writer, _, own) = terminal_v94(steps);
    let before = personal::sql(&s.fixture);
    let schema = schema_sql(&s);
    assert!(fresh_v94(&s, &own, &CancellationToken::default()).is_err());
    // The normal actual Personal reservation is intentionally outside this
    // unchanged evidence helper; every other raw table and schema is exact.
    assert_eq!(personal::sql(&s.fixture), before);
    assert_eq!(schema_sql(&s), schema);
    protected_v87(&s, "confirmed");
    transport_and_git_protocol(&s, Some(IssueLinkCommandV87::LinkIssue));
    drop(writer);
}
#[test]
fn pr_metadata_issue_link_v87_fresh_v94_actual_closing_pair_drift_stops_at13_without_uid_or_projection(
) {
    let mut steps = fresh_steps_v94();
    let mut pair: Value = serde_json::from_slice(&steps[12].stdout).unwrap();
    pair["data"]["pr"]["manual"] = connection(vec![]);
    pair["data"]["issue"]["manual"] = connection(vec![]);
    steps[12].stdout = serde_json::to_vec(&pair).unwrap();
    steps.truncate(13);
    let (s, writer, _, own) = terminal_v94(steps);
    let before = personal::sql(&s.fixture);
    let schema = schema_sql(&s);
    assert!(fresh_v94(&s, &own, &CancellationToken::default()).is_err());
    assert_eq!(personal::sql(&s.fixture), before);
    assert_eq!(schema_sql(&s), schema);
    protected_v87(&s, "confirmed");
    transport_and_git_protocol(&s, Some(IssueLinkCommandV87::LinkIssue));
    drop(writer);
}
