//! Four genuine real-binary sessions: actual Issue selection/Preview and idle
//! Close, then a SEPARATE current State producer/Confirm/source recovery.
//! All existing independent PRE-derived SQL/GH/Git/TTY checks remain in force.
use super::*;
const CAPABILITY_QUERY: &str="query IssueLinkCapabilityV87{__schema{mutationType{fields{name}}} add:__type(name:\"AddCloseIssueReferencesInput\"){kind inputFields{name type{kind name ofType{kind name ofType{kind name ofType{kind name}}}}}} remove:__type(name:\"RemoveCloseIssueReferencesInput\"){kind inputFields{name type{kind name ofType{kind name ofType{kind name ofType{kind name}}}}}} pr:__type(name:\"PullRequest\"){fields{name args{name type{kind name ofType{kind name}}}}} issue:__type(name:\"Issue\"){fields{name args{name type{kind name ofType{kind name}}}}}}";
const REPO_FIELDS: &str = "__typename id databaseId nameWithOwner isArchived viewerPermission";
const LINK_FIELDS:&str="__typename id fullDatabaseId number url repository{__typename id databaseId nameWithOwner isArchived viewerPermission}";
const PR_FIELDS:&str="__typename id fullDatabaseId number url title body headRefOid baseRefOid headRefName baseRefName state isDraft merged viewerCanUpdate repository{__typename id databaseId nameWithOwner isArchived viewerPermission}";
const ISSUE_FIELDS:&str="__typename id fullDatabaseId number url title state viewerCanUpdate repository{__typename id databaseId nameWithOwner isArchived viewerPermission}";
const CONTROLS: &[&str] = &["[Choose Issue]", "[Preview]", "[Esc Close]"];
const CLOSE_MUTATION_V79: &str = "mutation($id:ID!){closePullRequest(input:{pullRequestId:$id}){pullRequest{id fullDatabaseId number headRefOid baseRefOid headRefName baseRefName state isDraft merged repository{id nameWithOwner}}}}";

// Expected DATA only: neither a production draft encoder nor an origin/POST
// body readback supplies the exact desired key, selected value or envelope.
fn literal_issue_draft_v113() -> (String, String) {
    use sha2::{Digest, Sha256};
    let scope = json!({"host":HOST,"login":"user-id:42"});
    let stable = serde_json::to_vec(&(
        "ghtui-manual-issue-base-v87",
        &scope,
        10000u64,
        "REPO_catalogue_10000",
        FIRST,
        1007u64,
        "GENUINE_CANONICAL_PR_NODE_1007",
        7u64,
        10000u64,
        "REPO_catalogue_10000",
        FIRST,
        20009u64,
        "MANUAL_ISSUE_20009",
        9u64,
        "issue-link-v87",
    ))
    .unwrap();
    let base = format!("issue-link-base-v87/{:x}", Sha256::digest(stable));
    let root = serde_json::to_vec(&(
        "ghtui-manual-issue-draft-v87",
        base,
        "a".repeat(40),
        "b".repeat(40),
    ))
    .unwrap();
    let key = format!("issue-link-draft-v87/{:x}", Sha256::digest(root));
    let value = serde_json::to_string(&(
        87u8,
        "issue-link-v87",
        10000u64,
        FIRST,
        20009u64,
        "MANUAL_ISSUE_20009",
        9u64,
    ))
    .unwrap();
    let envelope = format!(
        "{{\"version\":17,\"values\":{{\"body\":{}}},\"publication\":null}}",
        serde_json::to_string(&value).unwrap(),
    );
    (key, envelope)
}

#[derive(Clone, Debug, PartialEq)]
struct IssueSaveImageV113 {
    schema: Vec<Vec<S>>,
    tables: SqlRows,
    file: (std::path::PathBuf, u64, u64, u32, u32, u64),
    user_version: i64,
    application_id: i64,
}
impl IssueSaveImageV113 {
    fn capture(f: &Fixture) -> Self {
        use std::os::unix::fs::MetadataExt;
        let named = fs::symlink_metadata(database(f)).unwrap();
        let path = fs::canonicalize(database(f)).unwrap();
        let file = fs::symlink_metadata(&path).unwrap();
        assert!(named.is_file() && file.is_file());
        assert_eq!((named.dev(), named.ino()), (file.dev(), file.ino()));
        assert_eq!(file.mode() & 0o777, 0o600);
        assert_eq!(file.nlink(), 1);
        let db = rusqlite::Connection::open_with_flags(
            &path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let tx = db.unchecked_transaction().unwrap();
        let rows = |sql: &str| {
            let mut statement = tx.prepare(sql).unwrap();
            let columns = statement.column_count();
            let mut rows = statement
                .query_map([], |row| {
                    (0..columns)
                        .map(|column| row.get::<_, S>(column))
                        .collect::<rusqlite::Result<Vec<_>>>()
                })
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            rows.sort_by_cached_key(|row| format!("{row:?}"));
            rows
        };
        let schema = rows(
            "SELECT type,name,tbl_name,rootpage,sql FROM sqlite_master ORDER BY type,name,tbl_name",
        );
        let mut tables = SqlRows::new();
        for row in &schema {
            if row[0] != t("table") {
                continue;
            }
            let S::Text(name) = &row[1] else {
                panic!("Actual SQLite table name must be text");
            };
            tables.insert(
                name.clone(),
                rows(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\""))),
            );
        }
        assert!(tables.contains_key("sqlite_sequence"));
        let captured = Self {
            schema,
            tables,
            file: (
                path,
                file.dev(),
                file.ino(),
                file.uid(),
                file.mode(),
                file.nlink(),
            ),
            user_version: tx
                .pragma_query_value(None, "user_version", |row| row.get(0))
                .unwrap(),
            application_id: tx
                .pragma_query_value(None, "application_id", |row| row.get(0))
                .unwrap(),
        };
        tx.commit().unwrap();
        captured
    }
    fn assert_saved(&self, f: &Fixture, earliest: u64, latest: u64) {
        let (key, body) = literal_issue_draft_v113();
        let actual = Self::capture(f);
        let own =
            |row: &&Vec<S>| row[0] == t(HOST) && row[1] == t("user-id:42") && row[2] == t(&key);
        assert_eq!(
            self.tables["drafts"].iter().filter(own).count(),
            0,
            "Fresh actual selection has no prior autosave or protected publication"
        );
        let saved = actual.tables["drafts"]
            .iter()
            .filter(own)
            .collect::<Vec<_>>();
        assert_eq!(saved.len(), 1, "Exactly the independently derived key");
        assert_eq!(saved[0].len(), 5);
        let S::Integer(timestamp) = &saved[0][4] else {
            panic!("Own durable Save timestamp is an actual integer");
        };
        let timestamp = *timestamp;
        assert!(timestamp >= 0 && (earliest..=latest).contains(&(timestamp as u64)));
        let mut expected = self.clone();
        expected.tables.get_mut("drafts").unwrap().push(vec![
            t(HOST),
            t("user-id:42"),
            t(&key),
            t(&body),
            S::Integer(timestamp),
        ]);
        expected
            .tables
            .get_mut("drafts")
            .unwrap()
            .sort_by_cached_key(|row| format!("{row:?}"));
        assert_eq!(
            actual, expected,
            "Full PRE-Preview image permits only the literal42 Issue Saved17 insertion; original startup/open reservations, schema, sequence, other accounts, private file and pragmas remain exact"
        );
    }
}

fn unix_seconds_v113() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
fn input_type(name: &str, required: bool) -> Value {
    let scalar = json!({"kind":"SCALAR","name":name,"ofType":null});
    if required {
        json!({"kind":"NON_NULL","name":null,"ofType":scalar})
    } else {
        scalar
    }
}
fn schema() -> Value {
    let fields = json!([
        {"name":"issueId","type":input_type("ID",true)},
        {"name":"pullRequestIds","type":{"kind":"NON_NULL","name":null,"ofType":{"kind":"LIST","name":null,"ofType":input_type("ID",true)}}},
        {"name":"clientMutationId","type":input_type("String",false)}
    ]);
    let args = |reverse| {
        let mut args = vec![
            json!({"name":"first","type":input_type("Int",false)}),
            json!({"name":"userLinkedOnly","type":input_type("Boolean",false)}),
            json!({"name":"excludeUserLinked","type":input_type("Boolean",false)}),
        ];
        if reverse {
            args.push(json!({"name":"includeClosedPrs","type":input_type("Boolean",false)}));
        }
        args
    };
    json!({"data":{"__schema":{"mutationType":{"fields":[{"name":"addCloseIssueReferences"},{"name":"removeCloseIssueReferences"}]}},
        "add":{"kind":"INPUT_OBJECT","inputFields":fields},"remove":{"kind":"INPUT_OBJECT","inputFields":fields},
        "pr":{"fields":[{"name":"closingIssuesReferences","args":args(false)}]},
        "issue":{"fields":[{"name":"closedByPullRequestsReferences","args":args(true)}]}}})
}
fn gql_issue_repository() -> Value {
    json!({"__typename":"Repository","id":"REPO_catalogue_10000","databaseId":10000,"nameWithOwner":FIRST,"isArchived":false,"viewerPermission":"WRITE"})
}
fn selected_issue() -> Value {
    json!({"__typename":"Issue","id":"MANUAL_ISSUE_20009","fullDatabaseId":20009,"number":9,
    "url":format!("https://{HOST}/{FIRST}/issues/9"),"title":"Actual Issue nine café 界","state":"OPEN","viewerCanUpdate":true,"repository":gql_issue_repository()})
}
fn connection(rows: Vec<Value>) -> Value {
    json!({"totalCount":rows.len(),"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":rows})
}
fn graphql(query: &str, variables: Value, value: Value) -> GhStep {
    let mut step = get("graphql", value);
    *step
        .args
        .iter_mut()
        .find(|arg| arg.as_str() == "GET")
        .unwrap() = "POST".into();
    step.args.extend(["--input".into(), "-".into()]);
    step.stdin = serde_json::to_vec(&json!({"query":query,"variables":variables})).unwrap();
    step
}
fn pr_node() -> Value {
    let raw = state_parent();
    json!({"__typename":"PullRequest","id":"GENUINE_CANONICAL_PR_NODE_1007","fullDatabaseId":1007,"number":7,
    "url":format!("https://{HOST}/{FIRST}/pull/7"),"title":raw["title"],"body":raw["body"],
    "headRefOid":raw["head"]["sha"],"baseRefOid":raw["base"]["sha"],"headRefName":raw["head"]["ref"],"baseRefName":raw["base"]["ref"],
    "state":"OPEN","isDraft":false,"merged":false,"viewerCanUpdate":true,"repository":gql_issue_repository(),
    "all":connection(Vec::new()),"manual":connection(Vec::new()),"nonmanual":connection(Vec::new())})
}
fn catalogue_query() -> String {
    [
    "query IssueLinkCatalogueV87($owner:String!,$name:String!,$after:String){repository(owner:$owner,name:$name){",REPO_FIELDS,
    " issues(first:50,after:$after,orderBy:{field:CREATED_AT,direction:DESC}){totalCount pageInfo{hasNextPage endCursor} nodes{",ISSUE_FIELDS,"}}}}"
].concat()
}
fn pr_links_query() -> String {
    let rows = |args: &str| {
        format!("closingIssuesReferences({args}first:100){{totalCount pageInfo{{hasNextPage endCursor}} nodes{{{LINK_FIELDS}}}}}")
    };
    format!("query IssueLinkPrReferencesV87($pr:ID!){{pr:node(id:$pr){{... on PullRequest{{{PR_FIELDS} all:{} manual:{} nonmanual:{}}}}}}}",rows(""),rows("userLinkedOnly:true,"),rows("excludeUserLinked:true,"))
}
fn issue_open_steps() -> Vec<GhStep> {
    let mut steps = principal(42);
    steps.push(get(&format!("repos/{FIRST}"), issue_repository()));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), state_parent()));
    steps.push(graphql(CAPABILITY_QUERY, json!({}), schema()));
    let mut repository = gql_issue_repository();
    repository["issues"] = connection(vec![selected_issue()]);
    steps.push(graphql(
        &catalogue_query(),
        json!({"owner":"example","name":"repo0000","after":null}),
        json!({"data":{"repository":repository}}),
    ));
    steps.push(graphql(
        &pr_links_query(),
        json!({"pr":"GENUINE_CANONICAL_PR_NODE_1007"}),
        json!({"data":{"pr":pr_node()}}),
    ));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), state_parent()));
    steps.push(get(&format!("repos/{FIRST}"), issue_repository()));
    steps.extend(principal(42));
    assert_eq!(steps.len(), 11);
    steps
}

fn pair_query() -> String {
    let connection = |field: &str, args: &str| {
        format!("{field}({args},first:100){{totalCount pageInfo{{hasNextPage endCursor}} nodes{{{LINK_FIELDS}}}}}")
    };
    format!("query IssueLinkPairV87($pr:ID!,$issue:ID!){{pr:node(id:$pr){{... on PullRequest{{{PR_FIELDS} all:{} manual:{} nonmanual:{}}}}} issue:node(id:$issue){{... on Issue{{{ISSUE_FIELDS} all:{} manual:{} nonmanual:{}}}}}}}",
        connection("closingIssuesReferences","" ).replacen("(,","(",1),
        connection("closingIssuesReferences","userLinkedOnly:true"),
        connection("closingIssuesReferences","excludeUserLinked:true"),
        connection("closedByPullRequestsReferences","includeClosedPrs:true"),
        connection("closedByPullRequestsReferences","includeClosedPrs:true,userLinkedOnly:true"),
        connection("closedByPullRequestsReferences","includeClosedPrs:true,excludeUserLinked:true"))
}
fn issue_query() -> String {
    format!("query IssueLinkSelectedV87($issue:ID!){{issue:node(id:$issue){{... on Issue{{{ISSUE_FIELDS}}}}}}}")
}

fn issue_repository() -> Value {
    let mut r = repository(0);
    r["permissions"] = json!({"pull":true,"push":true,"maintain":false,"admin":false});
    r
}
fn selected_issue_steps() -> Vec<GhStep> {
    let mut selected = selected_issue();
    for field in ["all", "manual", "nonmanual"] {
        selected[field] = connection(vec![]);
    }
    let observation = json!({"data":{"pr":pr_node(),"issue":selected}});
    let mut calls = principal(42);
    calls.push(get(&format!("repos/{FIRST}"), issue_repository()));
    calls.push(get(&format!("repos/{FIRST}/pulls/7"), state_parent()));
    calls.push(graphql(CAPABILITY_QUERY, json!({}), schema()));
    calls.push(get(&format!("repos/{FIRST}"), issue_repository()));
    calls.push(graphql(
        &pair_query(),
        json!({"pr":"GENUINE_CANONICAL_PR_NODE_1007","issue":"MANUAL_ISSUE_20009"}),
        observation.clone(),
    ));
    calls.push(graphql(
        &issue_query(),
        json!({"issue":"MANUAL_ISSUE_20009"}),
        json!({"data":{"issue":selected_issue()}}),
    ));
    calls.push(get(&format!("repos/{FIRST}"), issue_repository()));
    calls.push(graphql(
        &pair_query(),
        json!({"pr":"GENUINE_CANONICAL_PR_NODE_1007","issue":"MANUAL_ISSUE_20009"}),
        observation,
    ));
    calls.push(get(&format!("repos/{FIRST}/pulls/7"), state_parent()));
    calls.push(get(&format!("repos/{FIRST}"), issue_repository()));
    calls.extend(principal(42));
    assert_eq!(calls.len(), 14);
    calls
}
const PICKER: &str = "PR development links · Issue";
const PICKER_CONTROLS: &[&str] = &[
    "[Enter Choose]",
    "[Esc Back]",
    "[Ctrl-U Clear]",
    "[PgUp]",
    "[PgDn]",
];
const ISSUE_PREVIEW: &[&str] = &["[Preview]", "[Confirm action]", "[Keep draft / Close]"];
fn issue_preview_then_idle_close(
    s: &mut Session,
    f: &Fixture,
    mouse: bool,
    opened: usize,
    previewed: usize,
    steps: &[GhStep],
) {
    let title = format!("Link Issue • {FIRST}#7");
    let resized = s.completed_cells_end();
    s.resize(120, 40);
    fresh_cells(
        s,
        resized,
        |screen| {
            screen.size() == (40, 120)
                && crate::support::main_footer_visible(screen)
                && crate::support::controls::control_painted(screen, "[b Back]")
        },
        "NEW 120x40 actual original PR before Issue action",
    );
    let after = s.completed_cells_end();
    crate::pr_actions::menu(s, mouse, "Link Issue");
    s.wait_for_command_count(f, opened);
    form_frame(s, after, &title, CONTROLS);
    let initial =
        crate::support::overlay_body(s.completed_cells_screen().unwrap(), &title).unwrap();
    assert!(initial.contains("Selected actual Issue") && !initial.contains("[Confirm action]"));
    let before = all_rows(f);
    let commands = s.inspect_command_journal(f);
    let after = s.completed_cells_end();
    if mouse {
        let p = s.wait_for_owned_completed_action(&title, "[Choose Issue]", &[]);
        s.click(p.0, p.1);
    } else {
        s.send_key(b"\x1bOQ");
    }
    fresh_cells(
        s,
        after,
        |screen| {
            crate::support::overlay_body(screen, PICKER).is_some_and(|b| {
                PICKER_CONTROLS.iter().all(|c| b.contains(c))
                    && b.contains("Actual Issue nine café 界")
                    && b.contains("MANUAL_ISSUE_20009")
            })
        },
        "NEW genuine owned Issue picker with actual row and all controls",
    );
    s.capture_completed_terminal_v1("task79-cross-issue-picker");
    let selected_after;
    if mouse {
        let after = s.completed_cells_end();
        // The title also appears in the detail panel. Only the full actual
        // eligible row uniquely identifies the clickable row in this owner.
        let row = format!("◆ eligible · {FIRST}#9 • Actual Issue nine café 界");
        let p = s.wait_for_owned_completed_action(PICKER, &row, &[]);
        s.click(p.0, p.1);
        fresh_cells(
            s,
            after,
            |screen| {
                screen
                    .contents()
                    .contains("PR development links · Issue · Rows focused")
                    && crate::support::overlay_body(screen, PICKER)
                        .is_some_and(|b| PICKER_CONTROLS.iter().all(|c| b.contains(c)))
            },
            "NEW same-owned Issue Rows focus after actual row click",
        );
        selected_after = s.completed_cells_end();
        let p = s.wait_for_owned_completed_action(PICKER, "[Enter Choose]", &[]);
        s.click(p.0, p.1);
    } else {
        let after = s.completed_cells_end();
        s.send_key(b"\t");
        fresh_cells(
            s,
            after,
            |screen| {
                screen
                    .contents()
                    .contains("PR development links · Issue · Rows focused")
                    && crate::support::overlay_body(screen, PICKER)
                        .is_some_and(|b| PICKER_CONTROLS.iter().all(|c| b.contains(c)))
            },
            "ONE Tab produces NEW same-owned Issue Rows focus before Enter",
        );
        selected_after = s.completed_cells_end();
        s.send_key(b"\r");
    }
    fresh_cells(
        s,
        selected_after,
        |screen| {
            crate::support::overlay_body(screen, &title).is_some_and(|b| {
                CONTROLS.iter().all(|c| b.contains(c))
                    && b.contains("MANUAL_ISSUE_20009")
                    && !b.contains("[Confirm action]")
            })
        },
        "NEW same-owned actual chosen Issue Form before Preview",
    );
    assert_eq!(s.inspect_command_journal(f), commands);
    assert_eq!(
        all_rows(f),
        before,
        "Picker Choose is no SQL or publication"
    );
    let before_preview = IssueSaveImageV113::capture(f);
    let save_started = unix_seconds_v113();
    let after = s.completed_cells_end();
    if mouse {
        let p = s.wait_for_owned_completed_action(&title, "[Preview]", &[]);
        s.click(p.0, p.1);
    } else {
        s.send_key(b"\x10");
    }
    s.wait_for_command_count(f, previewed);
    form_frame(s, after, &title, ISSUE_PREVIEW);
    let body = crate::support::overlay_body(s.completed_cells_screen().unwrap(), &title).unwrap();
    assert!(body.contains("READ-ONLY PREVIEW") && body.contains("20009") && body.contains("1007"));
    let until = Instant::now() + Duration::from_secs(8);
    let issue_draft = loop {
        s.poll_output(Duration::from_millis(1));
        let rows=raw(f).prepare("SELECT resource,body FROM drafts WHERE host=?1 AND login='user-id:42' AND resource LIKE 'issue-link-draft-v87/%'").unwrap().query_map([HOST],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).unwrap().collect::<Result<Vec<_>,_>>().unwrap();
        if rows.len() == 1
            && s.completed_cells_screen().is_some_and(|screen| {
                crate::support::overlay_body(screen, &title).is_some()
                    && screen
                        .contents()
                        .contains("Draft saved locally • nothing published")
            })
        {
            break rows.into_iter().next().unwrap();
        }
        assert!(
            Instant::now() < until,
            "Real Issue initial Saved ACK before idle Close\n{}",
            s.screen()
        );
    };
    let (expected_key, expected_body) = literal_issue_draft_v113();
    assert_eq!(issue_draft, (expected_key, expected_body));
    before_preview.assert_saved(f, save_started, unix_seconds_v113());
    s.capture_completed_terminal_v1("task79-cross-issue-preview");
    assert_no_attempt(f);
    let stable = all_rows(f);
    let after = s.completed_cells_end();
    if mouse {
        let p = s.wait_for_owned_completed_action(&title, "[Keep draft / Close]", &[]);
        s.click(p.0, p.1);
    } else {
        s.send_key(b"\x1b");
    }
    fresh_cells(
        s,
        after,
        |screen| {
            crate::support::main_footer_visible(screen)
                && crate::support::controls::control_painted(screen, "[b Back]")
                && crate::support::overlay_body(screen, &title).is_none()
                && !screen.contents().contains("READ-ONLY PREVIEW")
        },
        "NEW actual original PR after idle Issue Close, not History or new Source",
    );
    assert_eq!(all_rows(f), stable);
    assert_no_attempt(f);
    exact(s, f, steps);
    let after = s.completed_cells_end();
    s.resize(80, 24);
    fresh_cells(
        s,
        after,
        |screen| {
            screen.size() == (24, 80)
                && crate::support::main_footer_visible(screen)
                && crate::support::controls::control_painted(screen, "[b Back]")
        },
        "NEW 80x24 original underlay before separate fresh State action",
    );
}
fn cross_family_journey(mouse: bool) {
    for personal in [false, true] {
        let mut steps = initial_steps(personal);
        let initial = steps.len();
        steps.extend(issue_open_steps());
        let issue_opened = steps.len();
        steps.extend(selected_issue_steps());
        let issue_previewed = steps.len();
        steps.extend(state_phase(json!(true)));
        let opened = steps.len();
        steps.extend(state_phase(json!(true)));
        let preview = steps.len();
        steps.extend(confirmed_tail());
        let confirmed = steps.len();
        let f = resource_configured(&steps);
        let sentinel_times = confirm_sql_v86::seed_preserved_rows(&f);
        let git = git_state(&f);
        let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
        let mut s = launch(&f, mouse);
        enter(
            &mut s,
            &f,
            personal,
            mouse,
            initial,
            false,
            &steps[..initial],
        );
        issue_preview_then_idle_close(
            &mut s,
            &f,
            mouse,
            issue_opened,
            issue_previewed,
            &steps[..issue_previewed],
        );
        let barrier = s.completed_cells_end();
        crate::pr_actions::menu(&mut s, mouse, "Close without merging");
        s.wait_for_command_count(&f, opened);
        state_form(&mut s, barrier, false);
        saved_bodyless(&mut s, &f);
        let barrier = s.completed_cells_end();
        control(&mut s, mouse, &state_title(), "[Preview]", STATE_INTENT);
        s.wait_for_command_count(&f, preview);
        state_form(&mut s, barrier, true);
        resize_state(&mut s, 120, 40, STATE_PREVIEW);
        let body =
            crate::support::overlay_body(s.completed_cells_screen().unwrap(), &state_title())
                .unwrap();
        assert!(
            body.contains("repository #10000")
                && body.contains("PR #1007")
                && body.contains("requested closed")
        );
        s.capture_completed_terminal_v1("task79-cross-state-preview");
        let confirm_sql = confirm_sql_v86::ConfirmSqlV86::capture(&f, personal, sentinel_times);
        control(
            &mut s,
            mouse,
            &state_title(),
            "[Confirm action]",
            STATE_PREVIEW,
        );
        s.wait_for_command_count(&f, confirmed);
        refreshed_v19::wait_fresh_page_v19(&mut s);
        let mut expected_sql = confirm_sql.assert_confirmed(&f);
        actual_terminal(&f);
        exact(&s, &f, &steps);
        let barrier = s.completed_cells_end();
        choose(&mut s, mouse, "[b Back]", b"b");
        fresh_cells(
            &mut s,
            barrier,
            |screen| {
                crate::support::main_footer_visible(screen)
                    && screen.contents().contains("counts unavailable")
                    && !crate::support::controls::control_painted(screen, "[b Back]")
                    && !screen.contents().contains(DETAIL_BODY)
            },
            "NEW exact State-unavailable Inbox after actual Back",
        );
        exact(&s, &f, &steps);
        assert_eq!(git_state(&f), git);
        assert_eq!(
            fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
            config
        );
        let source = source_name(&f);
        for _ in 0..2 {
            assert_eq!(
                all_rows(&f),
                expected_sql,
                "Actual Back/idle source changes no Confirm SQL"
            );
            confirm_sql.assert_auxiliary_unchanged(&f);
            let stable = expected_sql.clone();
            let catalogue_rows = stable["sync_reservations"]
                .iter()
                .filter(|r| r[0] == t(HOST) && r[1] == t("user-id:42"))
                .filter_map(|r| match &r[2] {
                    S::Text(s) if s.starts_with("repository-catalogue-source-v1:") => {
                        Some(s.clone())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(catalogue_rows.len(), usize::from(!personal));
            let catalogue = catalogue_rows.first().map(String::as_str);
            let generation = next_generation(&f, &source);
            let start = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            steps.extend(source_tail(personal));
            f.scripted_gh(&steps);
            let barrier = s.completed_cells_end();
            choose(&mut s, mouse, "[r Refresh]", b"r");
            s.wait_for_command_count(&f, steps.len());
            fresh_cells(
                &mut s,
                barrier,
                |screen| {
                    let text = screen.contents();
                    crate::support::main_footer_visible(screen)
                        && text.contains("Fresh complete State queue observation")
                        && !text.contains("counts unavailable")
                        && !text.contains("[Esc Cancel]")
                        && !crate::support::controls::control_painted(screen, "[b Back]")
                        && !text.contains(DETAIL_BODY)
                },
                "NEW genuine complete State Inbox after final principal",
            );
            let(g,fetched,complete):(u64,u64,bool)=raw(&f).query_row("SELECT generation,fetched_at,complete FROM checkpoints WHERE host=?1 AND login='user-id:42' AND source=?2",[HOST,source.as_str()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
            assert_eq!(g, generation);
            assert!(complete);
            let end = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            assert!((start..=end).contains(&fetched));
            expected_sql = expected_source(stable, &source, generation, fetched, catalogue);
            assert_eq!(all_rows(&f), expected_sql);
            confirm_sql.assert_auxiliary_unchanged(&f);
            actual_terminal(&f);
            exact(&s, &f, &steps);
            s.capture_completed_terminal_v1("task79-cross-source-recovered");
            assert_eq!(git_state(&f), git);
            assert_eq!(
                fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
                config
            );
        }
        let expected = graphql(
            CLOSE_MUTATION_V79,
            json!({"id":"GENUINE_CANONICAL_PR_NODE_1007"}),
            Value::Null,
        );
        let expected_input: Value = serde_json::from_slice(&expected.stdin).unwrap();
        let mut mutations = 0;
        for step in steps.iter().filter(|step| !step.stdin.is_empty()) {
            let input: Value = serde_json::from_slice(&step.stdin).unwrap();
            let document = input["query"].as_str().unwrap();
            match document
                .trim_start()
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .next()
                .unwrap()
            {
                "query" => {}
                "mutation" => {
                    assert_eq!(step.args, expected.args);
                    assert_eq!(input, expected_input);
                    mutations += 1;
                }
                other => panic!("Unexpected GraphQL operation {other:?}"),
            }
        }
        assert_eq!(
            mutations, 1,
            "Exactly one complete expected State write, not capability introspection"
        );
        assert_read_only_git(&f);
        s.quit_and_assert_restored_terminal(mouse);
        assert_eq!(
            all_rows(&f),
            expected_sql,
            "Actual Quit changes no derived SQL"
        );
        confirm_sql.assert_auxiliary_unchanged(&f);
    }
}
#[test]
fn cross_family_keyboard_issue_preview_close_then_actual_state_confirm_and_source_recovery() {
    cross_family_journey(false)
}
#[test]
fn cross_family_mouse_issue_preview_close_then_actual_state_confirm_and_source_recovery() {
    cross_family_journey(true)
}
