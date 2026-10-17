//! Actual read-only Background Metadata producer -> actual Issue publisher
//! terminal/OwnFence -> one NEW23-child Core. These are factory/code contracts,
//! not native evidence or a Source/permission reconstructed from fixture DTOs.
use super::*;
use crate::{
    app::View,
    github_workspace::PrTab,
    runtime::{scoped_inline_test_support_v1 as channel, Background},
    scoped_pr_collaboration_native_v17 as native,
};
use rusqlite::types::Value as SqlValue;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

fn context_parent_v101() -> Value {
    let mut raw = fresh_raw_v94();
    raw["maintainer_can_modify"] = json!(true);
    raw["labels"] = json!([{"id":90,"node_id":"LABEL_90","name":"Context label ☃"}]);
    raw["assignees"] = json!([{"id":60,"node_id":"U_60","login":"actual-assignee","type":"User"}]);
    raw["milestone"] = Value::Null;
    raw["requested_reviewers"] =
        json!([{"id":70,"node_id":"U_70","login":"actual-reviewer","type":"User"}]);
    raw["requested_teams"] =
        json!([{"id":75,"node_id":"T_75","name":"Actual team","slug":"actual-team"}]);
    raw["base"]["repo"] = writable_repository();
    raw
}
fn context_issue_v101() -> Value {
    let raw = context_parent_v101();
    json!({"id":2007,"node_id":"ISSUE_METADATA_2007","number":7,
        "url":format!("https://github.invalid/api/v3/repos/{}/issues/7",personal::REPO),
        "html_url":format!("https://github.invalid/{}/pull/7",personal::REPO),
        "repository_url":format!("https://github.invalid/api/v3/repos/{}",personal::REPO),
        "pull_request":{"url":raw["url"],"html_url":raw["html_url"]},
        "title":raw["title"],"body":raw["body"],"state":raw["state"],"user":raw["user"],
        "labels":raw["labels"],"assignees":raw["assignees"],"milestone":null,"locked":false})
}
fn context_four_v101() -> Vec<GhStep> {
    vec![
        personal::get(
            &format!("repos/{}/pulls/7", personal::REPO),
            context_parent_v101(),
        ),
        personal::get(
            &format!("repos/{}/issues/7", personal::REPO),
            context_issue_v101(),
        ),
        personal::get(&format!("repos/{}", personal::REPO), writable_repository()),
        personal::get(
            &format!("repos/{}/pulls/7", personal::REPO),
            context_parent_v101(),
        ),
    ]
}
fn section_v101() -> Vec<GhStep> {
    let mut calls = personal::principal();
    calls.push(personal::get(
        &format!("repos/{}", personal::REPO),
        writable_repository(),
    ));
    calls.extend(context_four_v101());
    calls.push(personal::get(
        &format!("repos/{}", personal::REPO),
        writable_repository(),
    ));
    calls.extend(personal::principal());
    assert_eq!(calls.len(), 10);
    calls
}
fn fresh_metadata_steps_v101() -> Vec<GhStep> {
    let mut calls = fresh_steps_v94();
    // SAME fixed profile: after actual five lists, before closing pair. No
    // discovery, extra window, provider getter or second Core is inserted.
    calls.splice(12..12, context_four_v101());
    assert_eq!(calls.len(), 23);
    calls
}
fn explicit_null_v101(raw: &mut Value) {
    match raw {
        Value::Object(values) => {
            if values.contains_key("body") {
                values.insert("body".into(), Value::Null);
            }
            for value in values.values_mut() {
                explicit_null_v101(value);
            }
        }
        Value::Array(values) => {
            for value in values {
                explicit_null_v101(value);
            }
        }
        _ => {}
    }
}
fn enter_metadata_v101(s: &mut personal::Setup) {
    let page = s.app.github.as_ref().unwrap().pr.as_ref().unwrap();
    assert_eq!(page.tab, PrTab::Summary);
    assert!(page.metadata_display_v51.is_none());
    native::capture_metadata_section_v51(&mut s.app, &personal::database(&s.fixture));
    let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
    let writer = DraftWriter::new(personal::database(&s.fixture));
    let mut background = Background::new();
    assert!(background.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        s.api.clone(),
        personal::database(&s.fixture),
        writer.client(),
    ));
    let until = Instant::now() + Duration::from_secs(8);
    while s.app.loading[View::Inbox.index()] {
        background.poll(&mut s.app);
        assert!(
            Instant::now() < until,
            "Genuine Metadata10 Background did not settle"
        );
        std::thread::yield_now();
    }
    assert!(channel::generation(&background).is_none());
    assert!(s.app.scoped_pr_collaboration_owner_v17.is_none());
    let page = s.app.github.as_ref().unwrap().pr.as_ref().unwrap();
    assert_eq!(page.tab, PrTab::Metadata);
    assert!(page.metadata_display_v51.is_some());
    assert!(page
        .rows
        .iter()
        .any(|row| row.label.contains("Context label ☃")));
    for (role, login) in [
        ("assigned", "actual-assignee"),
        ("review requested", "actual-reviewer"),
    ] {
        let actors = page
            .rows
            .iter()
            .filter(|row| row.label == role)
            .filter_map(|row| row.actor.as_ref())
            .collect::<Vec<_>>();
        assert_eq!(actors.len(), 1, "Exact actual Metadata actor role");
        assert_eq!(actors[0].login, login);
        assert_eq!(actors[0].actor_type, "User");
        assert!(actors[0].display_name.is_none() && actors[0].avatar_url.is_none());
    }
    assert!(page
        .rows
        .iter()
        .any(|row| row.label.contains("actual-team")));
    assert!(
        PrCollaborationOpenIntentV17::capture(&s.app, &personal::database(&s.fixture)).is_err()
    );
}
type OwnV101 = Arc<crate::storage::conversation_refresh_v19::ConversationOwnFenceV19>;
fn metadata_terminal_v101(
    later: Vec<GhStep>,
    body_null: bool,
) -> (personal::Setup, DraftWriter, ClosedReceiptV87, OwnV101) {
    let mut calls = section_v101();
    calls.extend(open_steps(
        catalogue(vec![issue(9)], false, None),
        pr_node(),
        personal::parent(),
    ));
    calls.extend(selected_steps(pair(pr_node(), issue(9))));
    calls.extend(selected_steps(pair(pr_node(), issue(9))));
    calls.extend(post_v87());
    calls.extend(later);
    if body_null {
        for call in &mut calls {
            // These are genuine wire responses. Only explicit body facts in
            // this closed task fixture change; identity/sets/roles stay exact.
            let mut raw: Value = serde_json::from_slice(&call.stdout).unwrap_or(Value::Null);
            if !raw.is_null() {
                explicit_null_v101(&mut raw);
                call.stdout = serde_json::to_vec(&raw).unwrap();
            }
        }
    }
    let mut s = personal::setup(calls);
    enter_metadata_v101(&mut s);
    let (writer, input) = publication_v87(&s);
    let closed = execute_receipt_v87(input).unwrap_or_else(|e| panic!("{}", e.message));
    assert!(closed.source.opening().issue_link_metadata_tab_v87());
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

#[derive(Clone, Debug, PartialEq)]
struct SqlImageV101 {
    schema: Vec<Vec<SqlValue>>,
    tables: BTreeMap<String, Vec<Vec<SqlValue>>>,
}
fn typed_rows_v101(db: &rusqlite::Connection, sql: &str) -> Vec<Vec<SqlValue>> {
    let mut query = db.prepare(sql).unwrap();
    let columns = query.column_count();
    let mut rows = query
        .query_map([], |row| {
            (0..columns)
                .map(|column| row.get(column))
                .collect::<rusqlite::Result<Vec<SqlValue>>>()
        })
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| format!("{row:?}"));
    rows
}
fn full_sql_v101(s: &personal::Setup) -> SqlImageV101 {
    let db = rusqlite::Connection::open_with_flags(
        personal::database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let schema = typed_rows_v101(
        &db,
        "SELECT type,name,tbl_name,rootpage,sql FROM sqlite_master",
    );
    let names = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    let tables = names
        .into_iter()
        .map(|name| {
            let rows = typed_rows_v101(
                &db,
                &format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")),
            );
            (name, rows)
        })
        .collect();
    SqlImageV101 { schema, tables }
}
fn literal_personal_source_v101() -> String {
    let scope = crate::storage::Scope::github("github.invalid", Some(42)).unwrap();
    let bytes = serde_json::to_vec(&(
        scope,
        10000u64,
        "R_10000",
        1007u64,
        "PERSONAL_PR_1007",
        7u64,
    ))
    .unwrap();
    format!("personal-pr-source-v13/{:x}", Sha256::digest(bytes))
}
fn expected_one_reservation_v101(before: &SqlImageV101) -> SqlImageV101 {
    let source = literal_personal_source_v101();
    let expected_key = [
        SqlValue::Text("github.invalid".into()),
        SqlValue::Text("user-id:42".into()),
        SqlValue::Text(source),
    ];
    let published = before
        .tables
        .get("checkpoints")
        .unwrap()
        .iter()
        .filter(|row| row[..3] == expected_key)
        .map(|row| match row[3] {
            SqlValue::Integer(n) => n,
            _ => panic!("Actual captured checkpoint integer generation"),
        })
        .max()
        .unwrap_or(0);
    let mut expected = before.clone();
    let rows = expected.tables.get_mut("sync_reservations").unwrap();
    let matching = rows
        .iter_mut()
        .filter(|row| row[..3] == expected_key)
        .collect::<Vec<_>>();
    assert_eq!(
        matching.len(),
        1,
        "Genuine original named reservation exists BEFORE fresh read"
    );
    let row = matching.into_iter().next().unwrap();
    assert_eq!(row.len(), 5);
    let reserved = match row[3] {
        SqlValue::Integer(n) if n >= 0 => n,
        _ => panic!("Actual captured reservation integer generation"),
    };
    row[3] = SqlValue::Integer(reserved.max(published).checked_add(1).unwrap());
    row[4] = SqlValue::Integer(1);
    rows.sort_by_key(|row| format!("{row:?}"));
    expected
}
fn prepare_metadata23_v101(
    s: &personal::Setup,
    own: &OwnV101,
    expected: &SqlImageV101,
    body_null: bool,
) -> GithubResult<
    crate::scoped_pr_collaboration_source_v17::initial_v17::ClosedRefreshReadV19<
        PreparedConversationPageV19,
    >,
> {
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    let attempt = own
        .begin_personal_read_attempt_v20(None, &provider, &cancel, deadline)
        .map_err(|_| refused(GithubErrorKind::IdentityChanged))?;
    with_issue_link_manual_refresh_v94(
        IssueLinkManualPhaseV94 {
            own,
            attempt: Some(&attempt),
        },
        &provider,
        own.terminal().database(),
        &cancel,
        deadline,
        |read| {
            assert_eq!(&full_sql_v101(s),expected,"No projection/body mutation BEFORE final UID; only independently modeled literal reservation");
            let ConversationPageSupplementV61::IssueLinksV87(context) = &read.page_supplement_v61
            else {
                panic!("Actual Issue Metadata supplement required")
            };
            assert!(context.matches_source_v87(own.source()));
            assert!(own.terminal().validates_issue_link_context_v87(context));
            assert_eq!(
                read.snapshot.details().summary.pr.body,
                if body_null { "" } else { personal::BODY }
            );
            let record = context.prepare_fixed_record_v87(&read.snapshot, own.terminal())?;
            assert_eq!(record.payload["observed"]["pr_body_null"], body_null);
            assert_eq!(record.payload["command"], "link_issue");
            assert_eq!(
                record.payload["observed"]["forward"][1]["20009"]["node"],
                "ISSUE_20009"
            );
            PreparedConversationPageV19::prepare_read_v19(own, read, &cancel)
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
    )
}
#[test]
fn pr_metadata_issue_link_v87_metadata_v101_actual_section_terminal_then_fresh23_prepares_real_page_and_keeps_raw_null_fact(
) {
    for body_null in [false, true] {
        let (s, writer, closed, own) =
            metadata_terminal_v101(fresh_metadata_steps_v101(), body_null);
        let before = full_sql_v101(&s);
        let expected = expected_one_reservation_v101(&before);
        let result = prepare_metadata23_v101(&s, &own, &expected, body_null)
            .unwrap_or_else(|e| panic!("{}", e.message));
        assert_eq!(
            full_sql_v101(&s),
            expected,
            "Actual23-child Core closure cannot commit the prepared page by itself"
        );
        let (_prepared, _identity, _seal) = result.into_material_v19();
        // The real prepared storage/page is intentionally not admitted by this
        // factory test. The typed close is genuine; no Source is inferred.
        assert_eq!(closed.source.opening().refresh_tab_v19(), PrTab::Metadata);
        protected_v87(&s, "confirmed");
        transport_and_git_protocol(&s, Some(IssueLinkCommandV87::LinkIssue));
        drop(writer);
    }
}
#[test]
fn pr_metadata_issue_link_v87_metadata_v101_final_uid43_preserves_full_sql_except_named_actual_reservation_and_no_projection(
) {
    let mut steps = fresh_metadata_steps_v101();
    steps[22].stdout = serde_json::to_vec(&json!({"id":43,"login":"different-account"})).unwrap();
    let (s, writer, _, own) = metadata_terminal_v101(steps, false);
    let expected = expected_one_reservation_v101(&full_sql_v101(&s));
    assert!(prepare_metadata23_v101(&s, &own, &expected, false).is_err());
    assert_eq!(full_sql_v101(&s), expected);
    protected_v87(&s, "confirmed");
    transport_and_git_protocol(&s, Some(IssueLinkCommandV87::LinkIssue));
    drop(writer);
}
#[test]
fn pr_metadata_issue_link_v87_metadata_v101_closing_pair_drift_after_context4_refuses_before_uid_full_sql_exact(
) {
    let mut steps = fresh_metadata_steps_v101();
    let mut pair: Value = serde_json::from_slice(&steps[16].stdout).unwrap();
    pair["data"]["pr"]["manual"] = connection(vec![]);
    pair["data"]["issue"]["manual"] = connection(vec![]);
    steps[16].stdout = serde_json::to_vec(&pair).unwrap();
    steps.truncate(17);
    let (s, writer, _, own) = metadata_terminal_v101(steps, false);
    let expected = expected_one_reservation_v101(&full_sql_v101(&s));
    assert!(prepare_metadata23_v101(&s, &own, &expected, false).is_err());
    assert_eq!(full_sql_v101(&s), expected);
    protected_v87(&s, "confirmed");
    transport_and_git_protocol(&s, Some(IssueLinkCommandV87::LinkIssue));
    drop(writer);
}
#[test]
fn pr_metadata_issue_link_v87_metadata_v101_foreign_issuer_refuses_before_new_core_or_reservation_with_full_sql_and_commands_exact(
) {
    let (s, writer, _, own) = metadata_terminal_v101(Vec::new(), false);
    let before = full_sql_v101(&s);
    let commands = fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let foreign = s
        .api
        .clone()
        .with_env("ISSUE_LINK_V101_FOREIGN_ISSUER", "different-config")
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    own.validate_before_read(&cancel, deadline).unwrap();
    assert!(own.source().admits_personal_refresh_producer_v20(&s.api));
    assert!(!own.source().admits_personal_refresh_producer_v20(&foreign));
    let result = own.begin_personal_read_attempt_v20(None, &foreign, &cancel, deadline);
    if let Ok(attempt) = result {
        assert!(with_issue_link_manual_refresh_v94(
            IssueLinkManualPhaseV94 {
                own: &own,
                attempt: Some(&attempt)
            },
            &foreign,
            own.terminal().database(),
            &cancel,
            deadline,
            |_| -> GithubResult<()> { panic!("Foreign issuer cannot prepare a Source or context") },
            |_, _| Ok(()),
        )
        .is_err());
    }
    assert_eq!(full_sql_v101(&s), before);
    assert_eq!(
        fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap(),
        commands
    );
    protected_v87(&s, "confirmed");
    transport_and_git_protocol(&s, Some(IssueLinkCommandV87::LinkIssue));
    drop(writer);
}
