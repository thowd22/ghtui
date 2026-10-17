//! Genuine original Personal source/Core/member fixtures. These direct worker
//! contracts are not claimed as native publication or all-ten product evidence.
use super::*;
use crate::{
    pr_metadata_actions::{PrMetadataCommand as Cmd, PrMetadataPlan},
    providers::gh::{OriginalMetadataMembersV48, PrMetadataReadLimits},
    scoped_pr_collaboration_source_v17::metadata_members_phase_v48::{
        self as members, MetadataMembersOpenV48,
    },
};
#[path = "scoped_pr_metadata_members_publication_v50_tests.rs"]
mod publication_v50;
fn repo() -> Value {
    let mut raw = repository();
    raw["permissions"]["push"] = json!(true);
    raw
}
fn label(id: u64, name: &str) -> Value {
    json!({"id":id,"node_id":format!("LABEL_{id}"),"name":name,"archived_at":null,"archived_by":null})
}
fn user(id: u64, name: &str) -> Value {
    json!({"id":id,"node_id":format!("USER_{id}"),"login":name,"type":"User","permissions":{"pull":true,"push":false}})
}
fn team(id: u64, name: &str) -> Value {
    json!({"id":id,"node_id":format!("TEAM_{id}"),"slug":name,"name":name,"permission":"pull","organization":{"login":"example"}})
}
fn milestone(id: u64, number: u64, name: &str) -> Value {
    json!({"id":id,"node_id":format!("MILESTONE_{id}"),"number":number,"title":name,"state":"open"})
}
fn raw() -> Value {
    let mut raw = parent();
    raw["base"]["repo"] = repo();
    raw["updated_at"] = json!("2026-10-16T04:00:00Z");
    raw["created_at"] = json!("2026-10-16T03:00:00Z");
    raw["labels"] = json!([label(10, "kept")]);
    raw["assignees"] = json!([user(20, "current-assignee")]);
    raw["requested_reviewers"] = json!([user(30, "current-reviewer")]);
    raw["requested_teams"] = json!([team(40, "current-team")]);
    raw["milestone"] = milestone(50, 1, "Current milestone");
    raw["maintainer_can_modify"] = json!(false);
    raw["additions"] = json!(1);
    raw["deletions"] = json!(1);
    raw["mergeable"] = Value::Null;
    raw["mergeable_state"] = json!("unknown");
    raw
}
fn issue(raw: &Value) -> Value {
    let mut issue = raw.clone();
    issue["id"] = json!(2007);
    issue["node_id"] = json!("ISSUE_2007");
    issue["pull_request"] = json!({"url":format!("https://github.invalid/api/v3/repos/{REPO}/pulls/7"),"html_url":format!("https://github.invalid/{REPO}/pull/7")});
    issue
}
fn candidate(command: Cmd) -> Value {
    match command.field() {
        "labels" => label(11, "new-label"),
        "assignees" => user(21, "new-assignee"),
        "reviewers" => user(31, "new-reviewer"),
        "teams" => {
            let mut row = team(41, "new-team");
            row.as_object_mut().unwrap().remove("organization");
            row
        }
        "milestone" => milestone(51, 2, "Next milestone"),
        _ => unreachable!(),
    }
}
fn query(command: Cmd, selected_team: bool) -> String {
    let selected=match command.field(){
        "labels"=>"labels(first:100){totalCount pageInfo{hasNextPage} nodes{id name}}",
        "assignees"=>"assignees(first:100){totalCount pageInfo{hasNextPage} nodes{id login}}",
        "milestone"=>"milestone{id number title}",
        "reviewers"|"teams"=>"reviewRequests(first:100){totalCount pageInfo{hasNextPage} nodes{id requestedReviewer{__typename ...on User{id login} ...on Team{id slug organization{login}}}}}",_=>unreachable!(),
    };
    let vars = if selected_team { ",$candidate:ID!" } else { "" };
    let alias = if selected_team {
        " candidate:node(id:$candidate){__typename ...on Team{id slug organization{login}}}"
    } else {
        ""
    };
    format!("query($id:ID!{vars}){{node(id:$id){{__typename ...on PullRequest{{id fullDatabaseId number headRefOid baseRefOid headRefName baseRefName state isDraft repository{{id nameWithOwner}} {selected}}}}}{alias}}}")
}
fn connection(rows: Vec<Value>) -> Value {
    json!({"totalCount":rows.len(),"pageInfo":{"hasNextPage":false},"nodes":rows})
}
fn current() -> Value {
    json!({"data":{"node":{"__typename":"PullRequest","id":"PERSONAL_PR_1007","fullDatabaseId":"1007","number":7,
    "headRefOid":"a".repeat(40),"baseRefOid":"b".repeat(40),"headRefName":"feature/personal","baseRefName":"main","state":"OPEN","isDraft":false,
    "repository":{"id":"R_10000","nameWithOwner":REPO},
    "labels":connection(vec![json!({"id":"LABEL_10","name":"kept"})]),
    "assignees":connection(vec![json!({"id":"USER_20","login":"current-assignee"})]),
    "milestone":{"id":"MILESTONE_50","number":1,"title":"Current milestone"},
    "reviewRequests":connection(vec![json!({"id":"REQUEST_USER_30","requestedReviewer":{"__typename":"User","id":"USER_30","login":"current-reviewer"}}),json!({"id":"REQUEST_TEAM_40","requestedReviewer":{"__typename":"Team","id":"TEAM_40","slug":"current-team","organization":{"login":"example"}}})])}}})
}
fn graphql(command: Cmd, value: Value, selected_team: bool) -> GhStep {
    let mut step = get("graphql", value);
    *step.args.iter_mut().find(|s| s.as_str() == "GET").unwrap() = "POST".into();
    step.args.extend(["--input".into(), "-".into()]);
    let mut variables = json!({"id":"PERSONAL_PR_1007"});
    if selected_team {
        variables["candidate"] = json!("TEAM_41");
    }
    step.stdin =
        serde_json::to_vec(&json!({"query":query(command,selected_team),"variables":variables}))
            .unwrap();
    step
}
fn phase(command: Cmd, raw: Value, choice: Value, gql: Value) -> Vec<GhStep> {
    let endpoint = match command.field() {
        "labels" => "labels",
        "assignees" => "assignees",
        "reviewers" => "collaborators",
        "teams" => "teams",
        "milestone" => "milestones?state=open",
        _ => unreachable!(),
    };
    let separator = if endpoint.contains('?') { '&' } else { '?' };
    let mut calls = principal();
    calls.push(get(&format!("repos/{REPO}"), repo()));
    calls.push(get(&format!("repos/{REPO}/pulls/7"), raw.clone()));
    calls.push(get(&format!("repos/{REPO}/issues/7"), issue(&raw)));
    calls.push(get(
        &format!("repos/{REPO}/{endpoint}{separator}per_page=100&page=1"),
        json!([choice]),
    ));
    calls.push(graphql(command, gql, false));
    calls.push(get(&format!("repos/{REPO}/pulls/7"), raw));
    calls.push(get(&format!("repos/{REPO}"), repo()));
    calls.extend(principal());
    assert_eq!(calls.len(), 11);
    calls
}
/// Strict test transport only: actual Core/member observation plus the real
/// five bounded detail children, then unchanged closing parent/Core children.
pub(crate) fn page_context_steps_v62(command: Cmd) -> Vec<GhStep> {
    let mut calls = phase(command, raw(), candidate(command), current());
    let lists = [
        ("issues/7/comments", json!([])),
        ("pulls/7/commits", json!([])),
        ("pulls/7/files", json!([{"filename":"src/view.rs","status":"modified","additions":1,"deletions":1,"patch":"@@ -1 +1 @@\n-old\n+new\n"}])),
        ("pulls/7/reviews", json!([])),
        ("issues/7/timeline", json!([])),
    ].into_iter().map(|(endpoint, rows)| {
        let mut step = get(&format!("repos/{REPO}/{endpoint}?per_page=100"), json!([rows]));
        step.args.extend(["--paginate".into(), "--slurp".into()]);
        step
    });
    calls.splice(7..7, lists);
    assert_eq!(calls.len(), 16);
    calls
}
type Opened = (
    Arc<source::PrCollaborationSourceV17>,
    Arc<OriginalMetadataMembersV48>,
    Result<PrMetadataPlan, String>,
);
fn open(
    s: &Setup,
    command: Cmd,
    deadline: Option<Instant>,
) -> crate::providers::gh::GithubResult<Opened> {
    let original = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline.unwrap_or(original.deadline()), 1024 * 1024)?;
    members::open_metadata_members_v48(
        MetadataMembersOpenV48 {
            opening: original.clone(),
            command,
            limits: &PrMetadataReadLimits::default(),
        },
        &api,
        &database(&s.fixture),
        original.cancellation(),
        original.deadline(),
        |actual, _| {
            Ok(PrMetadataPlan::prepare_original_members_v48(
                actual,
                if command == Cmd::ClearMilestone {
                    None
                } else {
                    Some(0)
                },
            ))
        },
    )
}
pub(crate) fn effects(s: &Setup, count: usize) {
    let successful_original = count == s.steps.len() && count > opening().len();
    effects_with_reservations_v79(s, count, usize::from(successful_original));
}
/// Pure Core context fixtures do not issue a Personal source/reservation. The
/// original Personal wrapper above retains its exact one-or-zero expectation.
pub(crate) fn effects_with_reservations_v79(s: &Setup, count: usize, expected: usize) {
    assert!(
        expected <= 1,
        "Only exact no-reservation or actual Personal fixture"
    );
    let rows = fs::read_to_string(s.fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), count);
    for (index, (row, expected)) in rows.iter().zip(&s.steps).enumerate() {
        assert_eq!(row["step"], index);
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(expected.args));
        assert_eq!(
            row["stdin_hex"],
            expected
                .stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        assert!(!expected
            .stdin
            .windows(8)
            .any(|window| window == b"mutation"));
    }
    assert_eq!(sql(&s.fixture), s.before_sql);
    // The shared legacy SQL observer deliberately excludes reservation rows.
    // Independently prove the exact reservation table, including absence for
    // the pure Core context fixture, not a full unchanged-SQL assertion.
    let db = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let reservations=db.prepare("SELECT host,login,source,generation,active FROM sync_reservations ORDER BY host,login,source").unwrap().query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,i64>(3)?,row.get::<_,i64>(4)?))).unwrap().map(Result::unwrap).collect::<Vec<_>>();
    assert_eq!(reservations.len(), expected);
    if let Some(row) = reservations.first() {
        assert_eq!(
            (&row.0, &row.1),
            (&"github.invalid".to_owned(), &"user-id:42".to_owned())
        );
        assert!(row.2.starts_with("personal-pr-source-v13/"));
        assert_eq!((row.3, row.4), (1, 1));
    }
    assert_eq!(git(&s.fixture, &["rev-parse", "HEAD"]), s.git[0]);
    for (path, expected) in [
        (".git/index", &s.git[1]),
        (".git/config", &s.git[2]),
        ("kept", &s.git[3]),
    ] {
        assert_eq!(&fs::read(s.fixture.repo().join(path)).unwrap(), expected);
    }
}
fn positive(command: Cmd) {
    let s = setup(phase(command, raw(), candidate(command), current()));
    let original_page = allocations(&s.app);
    let (source, actual, plan) = open(&s, command, None).unwrap();
    let plan = plan.unwrap();
    assert_eq!(plan.command(), command);
    assert_eq!(plan.repository_id(), 10000);
    assert_eq!(plan.pr_id(), 1007);
    assert_eq!(plan.issue_id(), 2007);
    assert_eq!(source.identity().account_id, Some(42));
    assert_eq!(actual.read().fetched_pages(), 1);
    assert_eq!(
        actual.read().accounting().requests(),
        1,
        "One eligible page is separately metered on the SAME original pool"
    );
    assert_eq!(actual.read().accounting().failed_reserved_bytes(), 0);
    assert!(actual.read().accounting().charged_bytes() <= 128 * 1024);
    assert_eq!(
        plan.preserved_review_members().len(),
        usize::from(matches!(command.field(), "reviewers" | "teams"))
    );
    assert_ne!(plan.before(), plan.after());
    assert_eq!(allocations(&s.app), original_page);
    assert!(
        actual.require_mutation_v48().is_ok(),
        "Retained data alone grants no later Core access; original clock is still live"
    );
    assert!(s.app.form.is_none());
    assert!(s.app.scoped_pr_collaboration_work_v17.is_none());
    effects(&s, s.steps.len());
}

fn full_sql_v290(s: &Setup) -> Vec<String> {
    let db = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let schema = db
        .prepare("SELECT type,name,COALESCE(sql,'') FROM sqlite_master ORDER BY type,name")
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    let mut all = Vec::new();
    for (kind, name, ddl) in schema {
        all.push(format!("schema:{kind}:{name}:{ddl}"));
        if kind != "table" {
            continue;
        }
        let mut statement = db
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        let count = statement.column_count();
        let mut rows = statement
            .query_map([], |row| {
                (0..count)
                    .map(|col| row.get::<_, rusqlite::types::Value>(col))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap()
            .map(Result::unwrap)
            .map(|row| format!("{name}:{row:?}"))
            .collect::<Vec<_>>();
        rows.sort();
        all.extend(rows);
    }
    all
}
fn original_capture_limit_v290(stdout: bool) {
    let mut calls = phase(Cmd::AddLabel, raw(), candidate(Cmd::AddLabel), current());
    assert_eq!(
        calls[5].args[1],
        format!("repos/{REPO}/labels?per_page=100&page=1")
    );
    if stdout {
        calls[5].stdout = b"PRIVATE_ORIGINAL_MEMBER_OUTPUT_CANARY".repeat(5000);
        assert!(calls[5].stdout.len() > 128 * 1024);
    } else {
        calls[5].stderr = b"\x01\x02".repeat(70_000);
        assert_eq!(calls[5].stderr.len(), 140_000);
    }
    calls.truncate(6);
    let s = setup(calls);
    let before = full_sql_v290(&s);
    let page = allocations(&s.app);
    let error = match open(&s, Cmd::AddLabel, None) {
        Err(error) => error,
        Ok(_) => panic!("Actual Original OutputLimit cannot issue a Source or prepared choices"),
    };
    assert!(matches!(
        error.kind,
        crate::providers::gh::GithubErrorKind::OutputLimit
    ));
    assert_eq!(
        error.message,
        "GitHub response exceeded its bounded capture; no response body admitted"
    );
    assert!(!error.repository_access_lost());
    assert!(!format!("{error:?} {error}").contains("PRIVATE_ORIGINAL_MEMBER_OUTPUT_CANARY"));
    assert_eq!(full_sql_v290(&s), before);
    assert_eq!(allocations(&s.app), page);
    assert!(s.app.form.is_none() && s.app.scoped_pr_collaboration_work_v17.is_none());
    effects_with_reservations_v79(&s, opening().len() + 6, 0);
}
#[test]
fn pr_metadata_members_v48_actual_original_stdout_limit_is_terminal_before_source_or_closing() {
    original_capture_limit_v290(true);
}
#[test]
fn pr_metadata_members_v48_actual_original_control_stderr_limit_is_terminal_before_source_or_closing(
) {
    original_capture_limit_v290(false);
}
#[test]
fn pr_metadata_members_v48_actual_original_empty_permission_and_transport_keep_distinct_accounting()
{
    use crate::providers::gh::{GithubErrorKind, MetricsReadStatus};
    use sha2::{Digest, Sha256};
    for mode in 0..3 {
        let mut calls = phase(Cmd::AddLabel, raw(), candidate(Cmd::AddLabel), current());
        if mode == 0 {
            calls[5].stdout = b"[]".to_vec();
        } else {
            calls[5].status = 1;
            calls[5].stdout.clear();
            calls[5].stderr = if mode == 1 {
                b"gh: Forbidden PRIVATE_ORIGINAL_MEMBER_403_CANARY (HTTP 403)\n".to_vec()
            } else {
                b"PRIVATE_ORDINARY_TRANSPORT_CANARY connection refused\n".to_vec()
            };
        }
        let s = setup(calls);
        let mut expected = full_sql_v290(&s);
        assert!(!expected
            .iter()
            .any(|row| row.starts_with("sync_reservations:")));
        let source = format!(
            "personal-pr-source-v13/{:x}",
            Sha256::digest(
                serde_json::to_vec(&(
                    json!({"host":"github.invalid","login":"user-id:42"}),
                    10000u64,
                    "R_10000",
                    1007u64,
                    "PERSONAL_PR_1007",
                    7u64
                ))
                .unwrap()
            )
        );
        assert_eq!(source,"personal-pr-source-v13/70014d4dd940fd87b9623122722d345014287e9c5628700049691aa23b64ae01");
        let table = expected
            .iter()
            .position(|row| row.starts_with("schema:table:sync_reservations:"))
            .unwrap();
        expected.insert(
            table + 1,
            format!(
                "sync_reservations:{:?}",
                [
                    rusqlite::types::Value::Text("github.invalid".into()),
                    rusqlite::types::Value::Text("user-id:42".into()),
                    rusqlite::types::Value::Text(source),
                    rusqlite::types::Value::Integer(1),
                    rusqlite::types::Value::Integer(1)
                ]
            ),
        );
        let page = allocations(&s.app);
        let (_, actual, plan) = open(&s, Cmd::AddLabel, None).unwrap();
        assert!(
            plan.is_err(),
            "Empty/unavailable cannot infer an eligible selection"
        );
        let read = actual.read();
        assert!(read.rows().is_empty());
        assert_eq!(read.accounting().requests(), 1);
        assert_eq!(
            read.accounting().failed_reserved_bytes(),
            if mode == 0 { 0 } else { 128 * 1024 }
        );
        if mode == 0 {
            assert!(read.coverage().complete_empty && read.coverage().enumeration_complete);
            assert!(matches!(
                read.coverage().status,
                MetricsReadStatus::Complete
            ));
        } else {
            assert!(!read.coverage().complete_empty && !read.coverage().enumeration_complete);
            let MetricsReadStatus::Unavailable(error) = &read.coverage().status else {
                panic!("Actual unavailable error required")
            };
            if mode == 1 {
                assert!(matches!(error.kind, GithubErrorKind::Permission));
                assert_eq!(
                    error.message,
                    "Member endpoint forbidden (HTTP 403); no empty catalogue inferred"
                );
                assert!(!format!("{:?}", read.coverage())
                    .contains("PRIVATE_ORIGINAL_MEMBER_403_CANARY"));
            } else {
                assert!(matches!(error.kind, GithubErrorKind::Transport));
                assert!(
                    error.message.contains("PRIVATE_ORDINARY_TRANSPORT_CANARY"),
                    "Ordinary Transport message behavior is unchanged"
                );
            }
        }
        assert_eq!(full_sql_v290(&s), expected);
        assert_eq!(allocations(&s.app), page);
        assert!(s.app.form.is_none());
        effects_with_reservations_v79(&s, s.steps.len(), 1);
    }
}
macro_rules! positive_case {
    ($name:ident,$command:ident) => {
        #[test]
        fn $name() {
            positive(Cmd::$command)
        }
    };
}
positive_case!(
    pr_metadata_members_v50_actual_add_label_index_and_distinct_issue_parent,
    AddLabel
);
positive_case!(
    pr_metadata_members_v50_actual_remove_label_current_id,
    RemoveLabel
);
positive_case!(
    pr_metadata_members_v50_actual_add_assignee_numeric_actor,
    AddAssignee
);
positive_case!(
    pr_metadata_members_v50_actual_remove_assignee_complete_current_set,
    RemoveAssignee
);
positive_case!(
    pr_metadata_members_v50_actual_add_user_reviewer_preserves_team,
    AddReviewer
);
positive_case!(
    pr_metadata_members_v50_actual_remove_user_reviewer_preserves_team,
    RemoveReviewer
);
positive_case!(
    pr_metadata_members_v50_actual_add_team_no_candidate_open_query,
    AddTeam
);
positive_case!(
    pr_metadata_members_v50_actual_remove_team_preserves_user,
    RemoveTeam
);
positive_case!(
    pr_metadata_members_v50_actual_set_actual_open_milestone,
    SetMilestone
);
positive_case!(
    pr_metadata_members_v50_actual_clear_current_milestone_no_candidate_index,
    ClearMilestone
);

#[test]
fn pr_metadata_members_v50_malformed_and_partial_graphql_never_mints_observation() {
    for mutation in 0..4 {
        let mut gql = current();
        match mutation {
            0 => gql["errors"] = json!({"message":"malformed"}),
            1 => gql["errors"] = json!([{"message":"partial"}]),
            2 => gql["data"]["node"]["labels"]["pageInfo"]["hasNextPage"] = json!(true),
            _ => gql["data"]["node"]["labels"]["totalCount"] = json!(2),
        };
        let s = setup(phase(Cmd::AddLabel, raw(), candidate(Cmd::AddLabel), gql));
        assert!(open(&s, Cmd::AddLabel, None).is_err());
        effects(&s, opening().len() + 7);
    }
}
#[test]
fn pr_metadata_members_v50_add_label_archive_absent_is_not_null_or_unarchived_permission() {
    for mode in 0..3 {
        let mut choice = candidate(Cmd::AddLabel);
        match mode {
            0 => {
                choice.as_object_mut().unwrap().remove("archived_at");
            }
            1 => choice["archived_at"] = json!("2026-10-16T00:00:00Z"),
            _ => {
                choice.as_object_mut().unwrap().remove("archived_by");
            }
        };
        let s = setup(phase(Cmd::AddLabel, raw(), choice, current()));
        let (_, _, plan) = open(&s, Cmd::AddLabel, None).unwrap();
        assert!(plan.is_err());
        effects(&s, s.steps.len());
    }
}
#[test]
fn pr_metadata_members_v50_remove_current_archived_label_preserves_remaining_archived_members() {
    let mut raw = raw();
    raw["labels"][0]["archived_at"] = json!("2026-10-15T00:00:00Z");
    raw["labels"][0]["archived_by"] = user(90, "archiver");
    let s = setup(phase(
        Cmd::RemoveLabel,
        raw,
        candidate(Cmd::RemoveLabel),
        current(),
    ));
    let (_, _, plan) = open(&s, Cmd::RemoveLabel, None).unwrap();
    assert!(plan.unwrap().after().is_empty());
    effects(&s, s.steps.len());
}
#[test]
fn pr_metadata_members_v50_team_namespace_key_requires_fresh_candidate_proof_not_rest_option() {
    let s = setup(phase(
        Cmd::AddTeam,
        raw(),
        candidate(Cmd::AddTeam),
        current(),
    ));
    let (_, actual, plan) = open(&s, Cmd::AddTeam, None).unwrap();
    let plan = plan.unwrap();
    assert!(actual.action_keys_v48(&plan).is_err(),"No candidate alias was requested by read-only Open, so namespace/key must wait for fresh Preview");
    effects(&s, s.steps.len());
}
#[test]
fn pr_metadata_members_v50_narrower_live_issuer_cutoff_is_retained_not_renewed() {
    let s = setup(phase(
        Cmd::AddAssignee,
        raw(),
        candidate(Cmd::AddAssignee),
        current(),
    ));
    let narrow = Instant::now() + Duration::from_secs(20);
    let (_, actual, plan) = open(&s, Cmd::AddAssignee, Some(narrow)).unwrap();
    assert!(plan.is_ok());
    assert!(actual.deadline() <= narrow);
    effects(&s, s.steps.len());
}
#[test]
fn pr_metadata_members_v50_expired_original_issuer_refuses_zero_new_children() {
    let s = setup(Vec::new());
    let expired = Instant::now() - Duration::from_millis(1);
    assert!(open(&s, Cmd::AddLabel, Some(expired)).is_err());
    effects(&s, s.steps.len());
}
