//! Actual binary Phase(a) original entry. These two real read/Close journeys
//! begin, not replace, the approved >=12 full mutation/recovery native matrix.
//! Pure route/fixture builders below are reused; no Members journey is called.
use super::*;
use rusqlite::types::Value as SqlValue;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const CAPABILITY_QUERY: &str="query IssueLinkCapabilityV87{__schema{mutationType{fields{name}}} add:__type(name:\"AddCloseIssueReferencesInput\"){kind inputFields{name type{kind name ofType{kind name ofType{kind name ofType{kind name}}}}}} remove:__type(name:\"RemoveCloseIssueReferencesInput\"){kind inputFields{name type{kind name ofType{kind name ofType{kind name ofType{kind name}}}}}} pr:__type(name:\"PullRequest\"){fields{name args{name type{kind name ofType{kind name}}}}} issue:__type(name:\"Issue\"){fields{name args{name type{kind name ofType{kind name}}}}}}";
const REPO_FIELDS: &str = "__typename id databaseId nameWithOwner isArchived viewerPermission";
const LINK_FIELDS:&str="__typename id fullDatabaseId number url repository{__typename id databaseId nameWithOwner isArchived viewerPermission}";
const PR_FIELDS:&str="__typename id fullDatabaseId number url title body headRefOid baseRefOid headRefName baseRefName state isDraft merged viewerCanUpdate repository{__typename id databaseId nameWithOwner isArchived viewerPermission}";
const ISSUE_FIELDS:&str="__typename id fullDatabaseId number url title state viewerCanUpdate repository{__typename id databaseId nameWithOwner isArchived viewerPermission}";
const CONTROLS: &[&str] = &["[Choose Issue]", "[Preview]", "[Esc Close]"];
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
fn gql_repo() -> Value {
    json!({"__typename":"Repository","id":"REPO_catalogue_10000","databaseId":10000,"nameWithOwner":FIRST,"isArchived":false,"viewerPermission":"WRITE"})
}
fn selected_issue() -> Value {
    json!({"__typename":"Issue","id":"MANUAL_ISSUE_20009","fullDatabaseId":20009,"number":9,
    "url":format!("https://{HOST}/{FIRST}/issues/9"),"title":"Actual Issue nine café 界","state":"OPEN","viewerCanUpdate":true,"repository":gql_repo()})
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
    let raw = raw();
    json!({"__typename":"PullRequest","id":"GENUINE_CANONICAL_PR_NODE_1007","fullDatabaseId":1007,"number":7,
    "url":format!("https://{HOST}/{FIRST}/pull/7"),"title":raw["title"],"body":raw["body"],
    "headRefOid":raw["head"]["sha"],"baseRefOid":raw["base"]["sha"],"headRefName":raw["head"]["ref"],"baseRefName":raw["base"]["ref"],
    "state":"OPEN","isDraft":false,"merged":false,"viewerCanUpdate":true,"repository":gql_repo(),
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
fn opened() -> Vec<GhStep> {
    let mut steps = principal(42);
    steps.push(get(&format!("repos/{FIRST}"), repo()));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), raw()));
    steps.push(graphql(CAPABILITY_QUERY, json!({}), schema()));
    let mut repository = gql_repo();
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
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), raw()));
    steps.push(get(&format!("repos/{FIRST}"), repo()));
    steps.extend(principal(42));
    assert_eq!(steps.len(), 11);
    steps
}
fn context4() -> Vec<GhStep> {
    vec![
        get(&format!("repos/{FIRST}/pulls/7"), raw()),
        get(&format!("repos/{FIRST}/issues/7"), issue(&raw())),
        get(&format!("repos/{FIRST}"), repo()),
        get(&format!("repos/{FIRST}/pulls/7"), raw()),
    ]
}
fn section() -> Vec<GhStep> {
    let mut steps = principal(42);
    steps.push(get(&format!("repos/{FIRST}"), repo()));
    steps.extend(context4());
    steps.push(get(&format!("repos/{FIRST}"), repo()));
    steps.extend(principal(42));
    assert_eq!(steps.len(), 10);
    steps
}
#[derive(Clone, Debug, PartialEq)]
struct Image {
    schema: Vec<Vec<SqlValue>>,
    tables: BTreeMap<String, Vec<Vec<SqlValue>>>,
}
fn rows(db: &rusqlite::Connection, query: &str) -> Vec<Vec<SqlValue>> {
    let mut q = db.prepare(query).unwrap();
    let columns = q.column_count();
    let mut rows = q
        .query_map([], |r| {
            (0..columns)
                .map(|i| r.get(i))
                .collect::<rusqlite::Result<Vec<SqlValue>>>()
        })
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    rows.sort_by_key(|r| format!("{r:?}"));
    rows
}
fn image(f: &Fixture) -> Image {
    let db = rusqlite::Connection::open_with_flags(
        database(f),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let schema = rows(
        &db,
        "SELECT type,name,tbl_name,rootpage,sql FROM sqlite_master",
    );
    let names = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    let tables = names
        .into_iter()
        .map(|name| {
            let data = rows(
                &db,
                &format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")),
            );
            (name, data)
        })
        .collect();
    Image { schema, tables }
}
fn expected_open(before: &Image, personal: bool) -> Image {
    let mut expected = before.clone();
    if !personal {
        return expected;
    }
    let scope = ghtui::storage::Scope::github(HOST, Some(42)).unwrap();
    let key = format!(
        "personal-pr-source-v13/{:x}",
        Sha256::digest(
            serde_json::to_vec(&(
                scope,
                10000u64,
                "REPO_catalogue_10000",
                1007u64,
                "GENUINE_CANONICAL_PR_NODE_1007",
                7u64
            ))
            .unwrap()
        )
    );
    let prefix = [
        SqlValue::Text(HOST.into()),
        SqlValue::Text("user-id:42".into()),
        SqlValue::Text(key),
    ];
    let published = before.tables["checkpoints"]
        .iter()
        .filter(|r| r[..3] == prefix)
        .map(|r| match r[3] {
            SqlValue::Integer(n) => n,
            _ => panic!("Known actual checkpoint integer"),
        })
        .max()
        .unwrap_or(0);
    let reservations = expected.tables.get_mut("sync_reservations").unwrap();
    let matches = reservations
        .iter_mut()
        .filter(|r| r[..3] == prefix)
        .collect::<Vec<_>>();
    assert!(matches.len() <= 1);
    if let Some(row) = matches.into_iter().next() {
        let generation = match row[3] {
            SqlValue::Integer(n) => n,
            _ => panic!("Known actual reservation integer"),
        };
        row[3] = SqlValue::Integer(generation.max(published).checked_add(1).unwrap());
        row[4] = SqlValue::Integer(1);
    } else {
        let mut row = prefix.to_vec();
        row.push(SqlValue::Integer(published.checked_add(1).unwrap()));
        row.push(SqlValue::Integer(1));
        reservations.push(row);
    }
    reservations.sort_by_key(|r| format!("{r:?}"));
    expected
}
fn frame(s: &mut Session, after: usize, title: &str) {
    s.wait_for_new_cursorless_condition(
        after,
        |frame| {
            crate::support::overlay_body(frame, title).is_some_and(|body| {
                CONTROLS.iter().all(|label| body.contains(label))
                    && body.contains("Selected actual Issue")
                    && !body.contains("[Confirm action]")
            })
        },
        "NEW exact same-owned initial Issue Form/controls",
        Duration::from_secs(8),
    );
}
fn original_conversation(s: &mut Session, mouse: bool) {
    let idle = |frame: &vt100::Screen| {
        let text = frame.contents();
        crate::support::main_footer_visible(frame)
            && crate::support::overlay_body(frame, "Inbox • focused view ─").is_some()
            && text.contains(&format!("{FIRST}#7"))
            && text.contains(&"a".repeat(40))
            && !text.contains("Verifying original Conversation")
            && !text.contains("Loading PR Metadata")
            && !text.contains("[Esc Cancel]")
    };
    let ready = |frame: &vt100::Screen| {
        idle(frame) && frame.contents().contains(OLD) && frame.contents().contains("comment #501")
    };
    if s.completed_cells_screen().is_some_and(ready) {
        return;
    }
    let until = std::time::Instant::now() + Duration::from_secs(8);
    let before = s.completed_cursorless_end();
    choose(s, mouse, "[c Conversation]", b"c");
    s.wait_for_new_cursorless_condition(
        before,
        |frame| {
            idle(frame)
                && (ready(frame)
                    || (frame
                        .contents()
                        .contains("@actual-pr-author PR description")
                        && frame.contents().contains("EXACT_PR_BODY_CANARY")))
        },
        "NEW actual idle original Conversation description before bounded scrolling",
        until.saturating_duration_since(std::time::Instant::now()),
    );
    // The actual fixture description precedes the original comment by sixty
    // rows. Supported input moves this SAME cached page; no additional read,
    // selected-comment authority, repeated tab shortcut or stale frame suffices.
    for _ in 0..60 {
        if s.completed_cursorless_screen().is_some_and(ready) {
            return;
        }
        let before = s.completed_cursorless_end();
        let prior = s.completed_cursorless_screen().unwrap().contents();
        if mouse {
            s.wheel(40, 20, true);
        } else {
            s.send_key(b"\x1b[6~");
        }
        s.wait_for_new_cursorless_condition(
            before,
            |frame| idle(frame) && frame.contents() != prior,
            "NEW same-owned original Conversation viewport after actual scroll",
            until.saturating_duration_since(std::time::Instant::now()),
        );
        assert!(
            std::time::Instant::now() < until,
            "Original comment not reached within original8s"
        );
    }
    panic!("Original comment missing after bounded actual Conversation scrolling");
}
fn original_summary(s: &mut Session, mouse: bool) {
    original_conversation(s, mouse);
    let before = s.completed_cursorless_end();
    choose(s, mouse, "[o Summary]", b"o");
    s.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let text = frame.contents();
            crate::support::main_footer_visible(frame)
                && text.contains(&format!("{FIRST}#7"))
                && text.contains(&"a".repeat(40))
                && !text.contains(OLD)
                && !text.contains("Verifying original Conversation")
                && !text.contains("Loading PR Metadata")
                && !text.contains("[Esc Cancel]")
        },
        "NEW actual Summary after distinct cached Conversation",
        Duration::from_secs(8),
    );
}
fn journey(mouse: bool) {
    for personal in [false, true] {
        for tab in ["Summary", "Conversation", "Metadata"] {
            for operation in ["Link Issue", "Unlink manual Issue"] {
                let mut steps = initial_steps(personal);
                let initial = steps.len();
                if tab == "Metadata" {
                    steps.extend(section());
                }
                let entered = steps.len();
                steps.extend(opened());
                let (f, mut s, git, config, _) = begin(&steps, personal, mouse, initial);
                // Fixture schema is established before any protected business oracle;
                // this is not app permission, an origin or a synthetic Source.
                ghtui::pr_write_journal::PrWriteJournal::open(&database(&f)).unwrap();
                s.resize(120, 40);
                s.wait_for_main_frame();
                original_summary(&mut s, mouse);
                if tab != "Summary" {
                    original_conversation(&mut s, mouse);
                }
                if tab == "Metadata" {
                    let before = s.completed_cursorless_end();
                    choose(&mut s, mouse, "[u Metadata]", b"u");
                    s.wait_for_command_count(&f, entered);
                    s.wait_for_new_cursorless_condition(
                        before,
                        |frame| {
                            crate::support::main_footer_visible(frame)
                                && frame.contents().contains("Your repository permissions:")
                                && frame
                                    .contents()
                                    .contains(&format!("https://{HOST}/{FIRST}/pull/7"))
                                && !frame.contents().contains("Verifying original Conversation")
                                && !frame.contents().contains("Loading PR Metadata")
                        },
                        "NEW genuine Metadata context before Issue action",
                        Duration::from_secs(8),
                    );
                }
                exact(&s, &f, &steps[..entered]);
                let before = image(&f);
                let expected = expected_open(&before, personal);
                let after = s.completed_cursorless_end();
                crate::pr_actions::menu(&mut s, mouse, operation);
                s.wait_for_command_count(&f, steps.len());
                let title = format!("{operation} • {FIRST}#7");
                frame(&mut s, after, &title);
                assert_eq!(image(&f),expected,"Only literal same-source Personal reservation may advance; every schema/typed table/sequence retained");
                let before = image(&f);
                let commands = s.inspect_command_journal(&f);
                // No Confirm is advertised. The real keyboard guard also refuses the
                // hidden shortcut in the mouse family without fabricating a hit.
                let guarded = s.completed_cursorless_end();
                s.send_key(b"\x1b[13;5u");
                s.wait_for_new_cursorless_condition(
                    guarded,
                    |frame| {
                        crate::support::overlay_body(frame, &title).is_some_and(|body| {
                            CONTROLS.iter().all(|label| body.contains(label))
                                && body.contains(
                                    "Fresh useful exact Issue Preview required before one Confirm",
                                )
                                && !body.contains("[Confirm action]")
                        })
                    },
                    "NEW SAME Issue Form acknowledges hidden Confirm refusal",
                    Duration::from_secs(8),
                );
                assert_eq!(s.inspect_command_journal(&f), commands);
                assert_eq!(image(&f), before);
                let after = s.completed_cursorless_end();
                choose(&mut s, mouse, "[Esc Close]", b"\x1b");
                s.wait_for_new_cursorless_condition(
                    after,
                    |frame| {
                        crate::support::main_footer_visible(frame)
                            && crate::support::overlay_body(frame, &title).is_none()
                    },
                    "NEW original page after pure Issue Close",
                    Duration::from_secs(8),
                );
                assert_eq!(image(&f), before);
                exact(&s, &f, &steps);
                assert_eq!(git_state(&f), git);
                assert_eq!(
                    fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
                    config
                );
                assert_read_only_git(&f);
                s.capture_completed_terminal_v1(
                    "issue-phase-a-actual-original-read-close-no-write",
                );
                s.quit_and_assert_restored_terminal(mouse);
            }
        }
    }
}
#[test]
fn pr_metadata_issue_link_v87_keyboard_entry_open_close_all_original_tabs_no_hidden_confirm_or_legacy_fallback(
) {
    journey(false)
}
#[test]
fn pr_metadata_issue_link_v87_mouse_entry_open_close_all_original_tabs_no_hidden_confirm_or_legacy_fallback(
) {
    journey(true)
}
