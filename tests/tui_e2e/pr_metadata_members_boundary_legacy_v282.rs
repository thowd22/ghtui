//! Original Members boundary journeys, not aliases to newer native tests.
//! Pure expected transport/owned completed-frame observers only; no production
//! Source, permission, receipt, acknowledgement or queued work is constructed.
use super::super::*;
use rusqlite::OptionalExtension;
use sha2::{Digest, Sha256};
#[path = "pr_metadata_members_label_query_legacy_v306.rs"]
pub(in crate::pr_actions::pr_metadata::metadata_actions) mod query_v306;
#[path = "pr_metadata_members_recovery_legacy_v294.rs"]
pub(in crate::pr_actions::pr_metadata::metadata_actions) mod recovery_v294;
const HOST: &str = "github.invalid";
const REPO: &str = "example/repo";
const PR_NODE: &str = "PR_7";
const LABEL_QUERY:&str="query($id:ID!){node(id:$id){__typename ...on PullRequest{id fullDatabaseId number headRefOid baseRefOid headRefName baseRefName state isDraft repository{id nameWithOwner} labels(first:100){totalCount pageInfo{hasNextPage} nodes{id name}}}}}";

fn repository_v282() -> Value {
    let mut repo = values::repository();
    repo["owner"] = json!({"id":310,"node_id":"O_310","login":"example","type":"Organization"});
    repo
}
fn original_v282() -> Value {
    let mut p = current_parent();
    p["url"] = json!("https://github.invalid/api/v3/repos/example/repo/pulls/7");
    p["base"]["repo"] = repository_v282();
    p["head"]["repo"] = repository_v282();
    for label in p["labels"].as_array_mut().unwrap() {
        label["archived_at"] = Value::Null;
        label["archived_by"] = Value::Null;
    }
    assert_eq!(p["id"], 1007);
    assert_eq!(p["node_id"], PR_NODE);
    assert_eq!(p["user"]["id"], 42);
    p
}
fn issue_v282(p: &Value) -> Value {
    let mut issue = values::issue_from(p);
    issue["url"] = json!("https://github.invalid/api/v3/repos/example/repo/issues/7");
    assert_ne!(issue["id"], p["id"]);
    assert_ne!(issue["node_id"], p["node_id"]);
    issue
}
fn principal_v282() -> Vec<GhStep> {
    vec![
        GhStep {
            args: vec!["--version".into()],
            stdout: b"gh version 2.99.0 (2026-10-01)\n".to_vec(),
            stdin: vec![],
            stderr: vec![],
            status: 0,
            delay_ms: 0,
        },
        values::get("user", json!({"id":42,"login":"requester"})),
    ]
}
fn section_v282(p: &Value) -> Vec<GhStep> {
    let mut calls = principal_v282();
    calls.push(values::get("repos/example/repo", repository_v282()));
    calls.extend([
        values::get("repos/example/repo/pulls/7", p.clone()),
        values::get("repos/example/repo/issues/7", issue_v282(p)),
        values::get("repos/example/repo", repository_v282()),
        values::get("repos/example/repo/pulls/7", p.clone()),
        values::get("repos/example/repo", repository_v282()),
    ]);
    calls.extend(principal_v282());
    assert_eq!(calls.len(), 10, "Actual Core6 plus four Metadata children");
    calls
}
fn startup_v282(p: &Value) -> Vec<GhStep> {
    let mut calls = bound(initial(), p);
    calls.extend(bound(details(vec![starter()]), p));
    assert_eq!(calls.len(), 14, "Actual initial7 plus detail7");
    for call in &mut calls {
        if call.args == ["--version"] {
            call.stdout = b"gh version 2.99.0 (2026-10-01)\n".to_vec();
        } else if call.args.get(1).is_some_and(|s| s == "repos/example/repo") {
            call.stdout = serde_json::to_vec(&repository_v282()).unwrap();
        }
    }
    calls.extend(section_v282(p));
    assert_eq!(calls.len(), 24);
    calls
}
fn label_open_v282(p: &Value) -> Vec<GhStep> {
    let command = PrMetadataCommand::AddLabel;
    let mut label = values::candidate(command);
    label["archived_at"] = Value::Null;
    label["archived_by"] = Value::Null;
    let mut calls = principal_v282();
    calls.extend([
        values::get("repos/example/repo", repository_v282()),
        values::get("repos/example/repo/pulls/7", p.clone()),
        values::get("repos/example/repo/issues/7", issue_v282(p)),
        values::get(
            "repos/example/repo/labels?per_page=100&page=1",
            json!([label]),
        ),
        values::graphql(
            LABEL_QUERY,
            json!({"id":PR_NODE}),
            values::members(command, p),
        ),
        values::get("repos/example/repo/pulls/7", p.clone()),
        values::get("repos/example/repo", repository_v282()),
    ]);
    calls.extend(principal_v282());
    assert_eq!(
        calls.len(),
        11,
        "SAME Core6 and actual rawPR/issue/page/GQL/closingraw"
    );
    calls
}
fn exact_v282(s: &mut Session, f: &Fixture, expected: &[GhStep], count: usize) -> Vec<Value> {
    s.wait_for_command_count(f, count);
    let until = Instant::now() + Duration::from_secs(8);
    let rows = loop {
        let raw = s.inspect_command_journal(f);
        let prefix = raw.rfind('\n').map_or("", |last| &raw[..=last]);
        let rows = prefix
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert!(
            rows.len() <= count,
            "Unexpected later child after exact current boundary"
        );
        if rows.len() == count {
            break rows;
        }
        assert!(
            Instant::now() < until,
            "Every expected strict line must finish"
        );
        s.poll_output(Duration::from_millis(1));
    };
    for (i, (actual, wanted)) in rows.iter().zip(&expected[..count]).enumerate() {
        assert_eq!(actual["step"], i);
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["args"], json!(wanted.args));
        assert_eq!(
            actual["stdin_hex"],
            wanted
                .stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
    }
    assert_eq!(mutation_count(&rows), 0);
    assert_no_publication(f);
    rows
}
fn whole_sql_v282(f: &Fixture) -> Vec<String> {
    let db = Connection::open_with_flags(cache_path(f), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
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
    for (kind, name, sql) in schema {
        all.push(format!("schema:{kind}:{name}:{sql}"));
        if kind != "table" {
            continue;
        }
        let mut statement = db
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        let columns = statement.column_count();
        let mut rows = statement
            .query_map([], |row| {
                (0..columns)
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
fn enter_metadata_v282(s: &mut Session, mouse: bool) {
    s.wait_for_main_frame();
    s.wait_for_screen("ghtui");
    // This fixture has its own actual PR title and requester. The ancestor
    // navigation helper belongs to the different Native collaboration PR.
    choose(s, mouse, "1 Inbox", b"1");
    s.wait_for_screen("Inbox • focused view");
    choose(s, mouse, "[r Refresh]", b"r");
    s.wait_for_screen("Unchanged title");
    s.wait_for_screen("Fresh • fetched");
    choose(s, mouse, "Unchanged title", b"\r");
    s.wait_for_screen("1 comments");
    let before = s.completed_cells_end();
    choose(s, mouse, "[c Conversation]", b"c");
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > before
            && s.completed_cells_screen().is_some_and(|frame| {
                let text = frame.contents();
                crate::support::main_footer_visible(frame)
                    // Full H belongs to this SAME completed frame's actual
                    // context in the content pane's top border, not the
                    // separately owned Conversation body.
                    && text.lines().any(|line| {
                        line.contains("Inbox • focused view")
                            && line.trim_end().ends_with(&format!(
                                "example/repo#7 / {HEAD} ─╮"
                            ))
                    })
                    && crate::support::overlay_body(frame, "Inbox • focused view ─").is_some_and(
                        |body| {
                            body.contains("example/repo#7")
                                && body.contains("@requester PR description")
                                && body.contains("Private description must never be in a receipt")
                                && body.contains("@octocat comment #9")
                                && body.contains("Existing starter comment")
                        },
                    )
                    && text.contains("[u Metadata]")
                    && !text.contains("[Esc Cancel]")
                    && !text.contains("Verifying original Conversation")
            })
        {
            break;
        }
        assert!(
            Instant::now() < until,
            "NEW completed genuine Conversation before Metadata\n{}",
            s.screen()
        );
    }
    let before = s.completed_cells_end();
    choose(s, mouse, "[u Metadata]", b"u");
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > before
            && s.completed_cells_screen().is_some_and(|frame| {
                let text = frame.contents();
                crate::support::main_footer_visible(frame)
                    && text.contains("[u Metadata]")
                    && text.contains("[p PR actions]")
                    && crate::support::overlay_body(frame, "Inbox • focused view ─").is_some_and(
                        |body| {
                            body.contains("example/repo#7")
                                && body
                                    .contains("Your repository permissions: read true • write true")
                                && body.contains("Locked conversation: false")
                                && body.contains("Milestone: Milestone 5")
                        },
                    )
                    && !text.contains("[Esc Cancel]")
            })
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "NEW actual Metadata context under original requester42\n{}",
            s.screen()
        );
    }
}
fn retired_v282(s: &mut Session, after: usize) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > after
            && s.completed_cells_screen().is_some_and(|frame| {
                let text = frame.contents();
                crate::support::main_footer_visible(frame)
                    && crate::support::overlay_body(frame, "Inbox • focused view ─").is_some_and(
                        |body| {
                            body.contains(
                            "Original Conversation retired • refresh explicitly • nothing resent",
                        )
                        },
                    )
                    && text.contains("Original Conversation unavailable")
                    && text.contains("nothing resent")
                    && ![
                        "[Choose member]",
                        "[Enter Choose]",
                        "[Confirm action]",
                        "[Esc Cancel]",
                        "[u Metadata]",
                        "Your repository permissions:",
                        "Selected value:",
                        "Private description",
                        "Unchanged title",
                        "existing ☃",
                        "new 🐛",
                        "current-reviewer",
                        "current-assignee",
                        "current-team",
                        "Milestone 5",
                        REPO,
                        HEAD,
                        "PR_7",
                        "L_81",
                    ]
                    .iter()
                    .any(|private| text.contains(private))
            })
        {
            return;
        }
        assert!(Instant::now()<until,"NEW intended same-login numeric-change refusal/private retirement, never empty/loading substitute\n{}",s.screen());
    }
}
pub(super) fn source_identity(mouse: bool) {
    let f = fixture();
    let p = original_v282();
    let head = f.git(&["rev-parse", "HEAD"]);
    let index = fs::read(f.repo().join(".git/index")).unwrap();
    let status = f.git(&["status", "--porcelain=v2", "-z"]);
    let config = fs::read(f.repo().join(".git/config")).unwrap();
    let mut steps = startup_v282(&p);
    let mut load = label_open_v282(&p);
    assert_eq!(load[1].args[1], "user");
    assert_eq!(
        serde_json::from_slice::<Value>(&load[1].stdout).unwrap(),
        json!({"id":42,"login":"requester"})
    );
    assert_eq!(load[10].args[1], "user");
    load[10].stdout = serde_json::to_vec(&json!({"id":43,"login":"requester"})).unwrap();
    steps.extend(load);
    assert_eq!(steps.len(), 35);
    f.scripted_gh(&steps);
    let mut s = Session::launch(&f, 120, 40);
    enter_metadata_v282(&mut s, mouse);
    exact_v282(&mut s, &f, &steps, 24);
    let before_sql = whole_sql_v282(&f);
    // The genuine same42 Personal read reserves its next generation before
    // the last UID refuses43. Model exactly that already-existing row; no
    // private source/data is installed for43 and every other row stays exact.
    let expected_sql = next_reservation_v290(before_sql, 1);
    let obsolete = crate::support::completed_navigation_controls_v1::owned_action_position(
        s.completed_cells_screen().unwrap(),
        "Inbox • focused view ─",
        "[u Metadata]",
        &[],
    )
    .expect("Actual original Metadata button before same-login numeric loss");
    let after = s.completed_cells_end();
    menu(&mut s, mouse, "Add label");
    s.wait_for_command_count(&f, 35);
    retired_v282(&mut s, after);
    let commands = exact_v282(&mut s, &f, &steps, 35);
    assert_eq!(whole_sql_v282(&f), expected_sql);
    let db =
        Connection::open_with_flags(cache_path(&f), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    for table in ["records", "drafts", "saved_views", "checkpoints"] {
        let count: u64 = db
            .query_row(
                &format!("SELECT count(*) FROM {table} WHERE host=?1 AND login='user-id:43'"),
                [HOST],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 0,
            "No previous private {table} copied under fresh numeric43"
        );
    }
    let after = s.completed_cells_end();
    if mouse {
        s.click(obsolete.0, obsolete.1);
    } else {
        s.send_key(b"u");
    }
    retired_v282(&mut s, after);
    assert_eq!(exact_v282(&mut s, &f, &steps, 35), commands);
    let after = s.completed_cells_end();
    s.resize(80, 24);
    retired_v282(&mut s, after);
    assert_eq!(whole_sql_v282(&f), expected_sql);
    assert_eq!(fs::read(f.repo().join(".git/config")).unwrap(), config);
    preserve_git(&f, &head, &index, &status);
    s.quit_and_assert_restored_terminal(mouse);
    assert_eq!(exact_v282(&mut s, &f, &steps, 35), commands);
    assert_eq!(whole_sql_v282(&f), expected_sql);
    preserve_git(&f, &head, &index, &status);
}

const REVIEW_FORM_V288: &str = "Remove user review request • example/repo#7";
const REVIEW_PICKER_V288: &str = "Current PR metadata members · Reviewer";
const REVIEW_QUERY_V288:&str="query($id:ID!){node(id:$id){__typename ...on PullRequest{id fullDatabaseId number headRefOid baseRefOid headRefName baseRefName state isDraft repository{id nameWithOwner} reviewRequests(first:100){totalCount pageInfo{hasNextPage} nodes{id requestedReviewer{__typename ...on User{id login} ...on Team{id slug organization{login}}}}}}}}";

fn reviewer_phase_v288(p: &Value) -> Vec<GhStep> {
    let mut calls = principal_v282();
    calls.extend([
        values::get("repos/example/repo", repository_v282()),
        values::get("repos/example/repo/pulls/7", p.clone()),
        values::get("repos/example/repo/issues/7", issue_v282(p)),
        values::get(
            "repos/example/repo/collaborators?per_page=100&page=1",
            json!([values::candidate(PrMetadataCommand::RemoveReviewer)]),
        ),
        values::graphql(
            REVIEW_QUERY_V288,
            json!({"id":PR_NODE}),
            values::members(PrMetadataCommand::RemoveReviewer, p),
        ),
        values::get("repos/example/repo/pulls/7", p.clone()),
        values::get("repos/example/repo", repository_v282()),
    ]);
    calls.extend(principal_v282());
    assert_eq!(
        calls.len(),
        11,
        "Actual Original Core6 plus five member reads"
    );
    calls
}
fn reviewer_draft_key_v288() -> String {
    // Independent expected codec for THIS literal selected original User,
    // never a production key getter or a serialized Source/permission.
    let hash = |raw: Vec<u8>| format!("{:x}", Sha256::digest(raw));
    let namespace: Option<&str> = None;
    let base = format!(
        "metadata-members-base-v48/{}",
        hash(
            serde_json::to_vec(&(
                "ghtui-metadata-members-base-v48",
                json!({"host":HOST,"login":"user-id:42"}),
                410u64,
                "R_410",
                1007u64,
                PR_NODE,
                7u64,
                2007u64,
                "I_2007",
                "metadata-remove-reviewer",
                70u64,
                "U_70",
                hash(serde_json::to_vec(&namespace).unwrap()),
            ))
            .unwrap()
        )
    );
    format!(
        "metadata-members-v48/{}",
        hash(
            serde_json::to_vec(&(
                "ghtui-metadata-members-draft-v48",
                base,
                "a".repeat(40),
                "b".repeat(40),
            ))
            .unwrap()
        )
    )
}
pub(super) fn opposite_review_drift(mouse: bool) {
    let f = fixture();
    let p = original_v282();
    let head = f.git(&["rev-parse", "HEAD"]);
    let index = fs::read(f.repo().join(".git/index")).unwrap();
    let status = f.git(&["status", "--porcelain=v2", "-z"]);
    let config = fs::read(f.repo().join(".git/config")).unwrap();
    let mut concurrent = p.clone();
    concurrent["requested_teams"] = json!([
        values::team(75, "current-team"),
        values::team(77, "concurrent-team")
    ]);
    assert_eq!(concurrent["requested_reviewers"], p["requested_reviewers"]);
    assert_eq!(concurrent["requested_reviewers"][0]["id"], 70);
    let mut steps = startup_v282(&p);
    steps.extend(reviewer_phase_v288(&p));
    let opened = steps.len();
    steps.extend(reviewer_phase_v288(&p));
    let previewed = steps.len();
    let mut refusal = reviewer_phase_v288(&concurrent);
    assert_eq!(refusal[3].args[1], "repos/example/repo/pulls/7");
    assert_eq!(
        serde_json::from_slice::<Value>(&refusal[3].stdout).unwrap()["requested_teams"],
        concurrent["requested_teams"]
    );
    assert_eq!(refusal[6].args[1], "graphql");
    let complete: Value = serde_json::from_slice(&refusal[6].stdout).unwrap();
    assert_eq!(complete["data"]["node"]["reviewRequests"]["totalCount"], 3);
    assert_eq!(
        complete["data"]["node"]["reviewRequests"]["pageInfo"]["hasNextPage"],
        false
    );
    assert!(complete["data"]["node"]["reviewRequests"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["requestedReviewer"]["id"] == "T_77"));
    // The SAME fresh observed REST+complete GraphQL set is internally valid,
    // then differs from the original Preview's preserved opposite Team set.
    // No closing raw/repository/final UID, claim or reference save follows.
    refusal.truncate(7);
    steps.extend(refusal);
    let refused = steps.len();
    assert_eq!((opened, previewed, refused), (35, 46, 53));
    f.scripted_gh(&steps);
    let mut s = Session::launch(&f, 120, 40);
    enter_metadata_v282(&mut s, mouse);
    let after = s.completed_cells_end();
    menu(&mut s, mouse, "Remove user review request");
    s.wait_for_command_count(&f, opened);
    ready_v283(&mut s, after, REVIEW_FORM_V288, SELECT_CONTROLS);
    exact_v282(&mut s, &f, &steps, opened);
    let before_selection = whole_sql_v282(&f);
    let after = s.completed_cells_end();
    control_v283(
        &mut s,
        mouse,
        REVIEW_FORM_V288,
        "[Choose member]",
        SELECT_CONTROLS,
    );
    ready_v283(
        &mut s,
        after,
        REVIEW_PICKER_V288,
        &[
            "Current reviewer: @current-reviewer (account #70)",
            "[Enter Choose]",
        ],
    );
    control_v283(
        &mut s,
        mouse,
        REVIEW_PICKER_V288,
        "[Enter Choose]",
        PICKER_CONTROLS,
    );
    s.wait_for_overlay_close(REVIEW_PICKER_V288);
    s.wait_for_owned_completed_action(REVIEW_FORM_V288, "[Preview]", &[]);
    let selected = "current-reviewer • actual #70 • U_70";
    assert!(
        crate::support::overlay_body(s.completed_cells_screen().unwrap(), REVIEW_FORM_V288)
            .unwrap()
            .contains(selected)
    );
    assert_eq!(whole_sql_v282(&f), before_selection);
    let key = reviewer_draft_key_v288();
    assert_eq!(saved_v283(&f, &key), None);
    let stable = unrelated_sql_v283(&f, &key);
    let after = s.completed_cells_end();
    control_v283(
        &mut s,
        mouse,
        REVIEW_FORM_V288,
        "[Preview]",
        SELECT_CONTROLS,
    );
    s.wait_for_command_count(&f, previewed);
    ready_v283(
        &mut s,
        after,
        REVIEW_FORM_V288,
        &[
            "READ-ONLY PREVIEW",
            "Exact effect: Remove user review request",
            "U_70",
            "[Confirm action]",
        ],
    );
    let expected = json!({"version":17,"values":{"body":selected},"publication":null});
    s.try_wait_for_condition(
        |_| {
            saved_v283(&f, &key)
                .is_some_and(|body| serde_json::from_str::<Value>(&body).unwrap() == expected)
        },
        "Exact current User70 inert v17 autosave settles before Confirm",
        Duration::from_secs(8),
    )
    .unwrap();
    assert_eq!(unrelated_sql_v283(&f, &key), stable);
    let durable = saved_v283(&f, &key).unwrap();
    let before_confirm = whole_sql_v282(&f);
    exact_v282(&mut s, &f, &steps, previewed);
    let after = s.completed_cells_end();
    control_v283(
        &mut s,
        mouse,
        REVIEW_FORM_V288,
        "[Confirm action]",
        PREVIEW_CONTROLS,
    );
    s.wait_for_command_count(&f, refused);
    retired_v282(&mut s, after);
    assert!(!s
        .completed_cells_screen()
        .unwrap()
        .contents()
        .contains("concurrent-team"));
    let commands = exact_v282(&mut s, &f, &steps, refused);
    assert_eq!(saved_v283(&f, &key), Some(durable.clone()));
    assert_eq!(whole_sql_v282(&f), before_confirm);
    let after = s.completed_cells_end();
    s.resize(80, 24);
    retired_v282(&mut s, after);
    assert_eq!(whole_sql_v282(&f), before_confirm);
    assert_eq!(saved_v283(&f, &key), Some(durable));
    assert_eq!(fs::read(f.repo().join(".git/config")).unwrap(), config);
    preserve_git(&f, &head, &index, &status);
    s.quit_and_assert_restored_terminal(mouse);
    assert_eq!(exact_v282(&mut s, &f, &steps, refused), commands);
    assert_eq!(whole_sql_v282(&f), before_confirm);
    preserve_git(&f, &head, &index, &status);
}

fn unknown_metadata_v288(s: &mut Session, after: usize, body: Option<&str>, failed: bool) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > after
            && s.completed_cells_screen().is_some_and(|frame| {
                let text = frame.contents();
                let (rows, columns) = frame.size();
                crate::support::main_footer_visible(frame)
                    && crate::support::overlay_body(frame, "Inbox • focused view ─").is_some_and(|actual| {
                        actual.contains("example/repo#7")
                            && actual.contains("Your repository permissions: read true • write true")
                            && actual.contains("Milestone: Milestone 5")
                            && body.is_none_or(|expected| actual == expected)
                    })
                    && (!failed || frame.contents_between(rows.saturating_sub(2), 0, rows.saturating_sub(2), columns).trim_end()
                        == "Original Conversation unavailable • nothing resent • draft/journal retained")
                    && text.contains("[u Metadata]")
                    && !["Current PR metadata members", "PR metadata choices", "[Choose member]", "[Enter Choose]", "[Confirm action]", "[Esc Cancel]", "[Clear milestone]", "[Keep draft / Close]"]
                        .iter().any(|forbidden| text.contains(forbidden))
            })
        {
            return;
        }
        assert!(Instant::now() < until,
            "NEW completed original Metadata; null current set grants no empty/clear/catalogue/Source/Form\n{}", s.screen());
    }
}
pub(in crate::pr_actions::pr_metadata::metadata_actions) fn missing_current_array(mouse: bool) {
    let f = fixture();
    let p = original_v282();
    let head = f.git(&["rev-parse", "HEAD"]);
    let index = fs::read(f.repo().join(".git/index")).unwrap();
    let status = f.git(&["status", "--porcelain=v2", "-z"]);
    let config = fs::read(f.repo().join(".git/config")).unwrap();
    let mut absent = p.clone();
    absent["labels"] = Value::Null;
    assert_eq!(p["labels"].as_array().unwrap().len(), 1);
    assert!(absent["labels"].is_null() && !absent["labels"].is_array());
    let mut read = label_open_v282(&p);
    read[3].stdout = serde_json::to_vec(&absent).unwrap();
    read[4].stdout = serde_json::to_vec(&issue_v282(&absent)).unwrap();
    assert_eq!(read[3].args[1], "repos/example/repo/pulls/7");
    assert_eq!(read[4].args[1], "repos/example/repo/issues/7");
    assert!(serde_json::from_slice::<Value>(&read[4].stdout).unwrap()["labels"].is_null());
    assert_eq!(
        serde_json::from_slice::<Value>(&read[5].stdout)
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );
    // A successful nonempty eligible REST page is not evidence that the NULL
    // current set is empty. current_members rejects before current GQL, all
    // closing reads, parent reservation and any editable/choice Form.
    read.truncate(6);
    let mut steps = startup_v282(&p);
    steps.extend(read);
    assert_eq!(steps.len(), 30);
    f.scripted_gh(&steps);
    let mut s = Session::launch(&f, 120, 40);
    enter_metadata_v282(&mut s, mouse);
    exact_v282(&mut s, &f, &steps, 24);
    let before = whole_sql_v282(&f);
    let original = crate::support::overlay_body(
        s.completed_cells_screen().unwrap(),
        "Inbox • focused view ─",
    )
    .unwrap();
    let after = s.completed_cells_end();
    menu(&mut s, mouse, "Remove label");
    s.wait_for_command_count(&f, 30);
    unknown_metadata_v288(&mut s, after, Some(&original), true);
    let commands = exact_v282(&mut s, &f, &steps, 30);
    assert_eq!(whole_sql_v282(&f), before);
    let after = s.completed_cells_end();
    // No Confirm control exists to click. The old global Confirm shortcut is
    // still probed in both genuine sessions, and must create no Source/work.
    s.send_key(b"\x1b[13;5u");
    unknown_metadata_v288(&mut s, after, Some(&original), false);
    assert_eq!(exact_v282(&mut s, &f, &steps, 30), commands);
    assert_eq!(whole_sql_v282(&f), before);
    let after = s.completed_cells_end();
    s.resize(80, 24);
    unknown_metadata_v288(&mut s, after, None, false);
    assert_eq!(whole_sql_v282(&f), before);
    let after = s.completed_cells_end();
    s.resize(120, 40);
    unknown_metadata_v288(&mut s, after, Some(&original), false);
    assert_eq!(whole_sql_v282(&f), before);
    assert_eq!(fs::read(f.repo().join(".git/config")).unwrap(), config);
    preserve_git(&f, &head, &index, &status);
    s.quit_and_assert_restored_terminal(mouse);
    assert_eq!(exact_v282(&mut s, &f, &steps, 30), commands);
    assert_eq!(whole_sql_v282(&f), before);
    preserve_git(&f, &head, &index, &status);
}

const LABEL_PICKER: &str = "PR metadata choices · Label";
const LABEL_FORM: &str = "Add label • example/repo#7";
const PICKER_CONTROLS: &[&str] = &[
    "[Enter Choose]",
    "[Esc Back]",
    "[Ctrl-U Clear]",
    "[PgUp]",
    "[PgDn]",
];
const SELECT_CONTROLS: &[&str] = &["[Choose member]", "[Preview]", "[Esc Close]"];
const PREVIEW_CONTROLS: &[&str] = &["[Preview]", "[Confirm action]", "[Keep draft / Close]"];
fn ready_v283(s: &mut Session, after: usize, title: &str, needles: &[&str]) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > after
            && s.completed_cells_screen().is_some_and(|frame| {
                crate::support::main_footer_visible(frame)
                    && crate::support::overlay_body(frame, title).is_some_and(|body| {
                        needles.iter().all(|n| body.contains(n)) && !body.contains("[Esc Cancel]")
                    })
            })
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "NEW same-owned {title} with complete expected controls/facts\n{}",
            s.screen()
        );
    }
}
fn control_v283(s: &mut Session, mouse: bool, title: &str, label: &str, controls: &[&str]) {
    if !mouse {
        let titles = [
            format!("{title} · Filter focused"),
            format!("{title} · Rows focused"),
        ];
        let title_refs = titles.each_ref().map(String::as_str);
        let notices = if title == LABEL_PICKER || title == REVIEW_PICKER_V288 {
            title_refs.as_slice()
        } else {
            &[]
        };
        s.focus_completed_owned_action_v1(
            title,
            label,
            crate::support::completed_navigation_controls_v1::FocusAttributeV1::Inverse,
            controls,
            notices,
        );
    }
    let frame = s
        .completed_cells_screen()
        .expect("Actual completed topmost Members control");
    let (x, y) = crate::support::completed_navigation_controls_v1::owned_action_position(
        frame,
        title,
        label,
        &[],
    )
    .expect("Whole owned control before actual input");
    if mouse {
        s.click(x, y);
    } else {
        assert!(frame.cell(y, x).unwrap().inverse());
        s.send_key(b"\r");
    }
}
fn selected_draft_key_v283() -> String {
    let hash = |raw: Vec<u8>| format!("{:x}", Sha256::digest(raw));
    let namespace: Option<&str> = None;
    let base = format!(
        "metadata-members-base-v48/{}",
        hash(
            serde_json::to_vec(&(
                "ghtui-metadata-members-base-v48",
                Scope::github(HOST, Some(42)).unwrap(),
                410u64,
                "R_410",
                1007u64,
                PR_NODE,
                7u64,
                2007u64,
                "I_2007",
                "metadata-add-label",
                81u64,
                "L_81",
                hash(serde_json::to_vec(&namespace).unwrap())
            ))
            .unwrap()
        )
    );
    format!(
        "metadata-members-v48/{}",
        hash(
            serde_json::to_vec(&(
                "ghtui-metadata-members-draft-v48",
                base,
                "a".repeat(40),
                "b".repeat(40)
            ))
            .unwrap()
        )
    )
}
fn saved_v283(f: &Fixture, key: &str) -> Option<String> {
    Connection::open_with_flags(cache_path(f),rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap()
        .query_row("SELECT body FROM drafts WHERE host='github.invalid' AND login='user-id:42' AND resource=?1",[key],|row|row.get(0)).optional().unwrap()
}
fn unrelated_sql_v283(f: &Fixture, key: &str) -> Vec<String> {
    // Exactly ONE independently derived current own draft is allowed to be
    // newly saved after actual Preview. Every other table/schema/row stays.
    let exact = format!("drafts:[Text({HOST:?}), Text(\"user-id:42\"), Text({key:?}),");
    whole_sql_v282(f)
        .into_iter()
        .filter(|row| !row.starts_with(&exact))
        .collect()
}

fn failed_open_v290(s: &mut Session, after: usize, expected: Option<&str>, failed: bool) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > after && s.completed_cells_screen().is_some_and(|frame| {
            let text=frame.contents();let (rows,columns)=frame.size();
            crate::support::main_footer_visible(frame)
                && crate::support::overlay_body(frame,"Inbox • focused view ─").is_some_and(|body|
                    body.contains("example/repo#7") && body.contains("Your repository permissions: read true • write true")
                        && expected.is_none_or(|original|body==original))
                && (!failed || frame.contents_between(rows.saturating_sub(2),0,rows.saturating_sub(2),columns).trim_end()
                    == "Original Conversation unavailable • nothing resent • draft/journal retained")
                && text.contains("[u Metadata]")
                && ![LABEL_FORM,LABEL_PICKER,"[Choose member]","[Enter Choose]","[Confirm action]","[Esc Cancel]","[Keep draft / Close]","Complete catalogue:","PRIVATE_MEMBER_CAPTURE_CANARY"]
                    .iter().any(|forbidden|text.contains(forbidden))
        }) {return;}
        assert!(Instant::now()<until,"NEW completed failed Original member opening, no catalogue/Source/Form or raw diagnostic\n{}",s.screen());
    }
}
pub(super) fn raw_capture_limit(mouse: bool) {
    let f = fixture();
    let p = original_v282();
    let head = f.git(&["rev-parse", "HEAD"]);
    let index = fs::read(f.repo().join(".git/index")).unwrap();
    let status = f.git(&["status", "--porcelain=v2", "-z"]);
    let config = fs::read(f.repo().join(".git/config")).unwrap();
    let mut steps = startup_v282(&p);
    let mut read = label_open_v282(&p);
    assert_eq!(
        read[5].args[1],
        "repos/example/repo/labels?per_page=100&page=1"
    );
    read[5].stdout.clear();
    read[5].status = 1;
    read[5].stderr = b"PRIVATE_MEMBER_CAPTURE_CANARY".to_vec();
    let stderr_len = read[5].stderr.len() + 128 * 1024 + 1;
    read[5].stderr.resize(stderr_len, b'\x1b');
    assert!(read[5].stderr.len() > 128 * 1024);
    read.truncate(6);
    steps.extend(read);
    assert_eq!(steps.len(), 30);
    f.scripted_gh(&steps);
    let mut s = Session::launch(&f, 120, 40);
    enter_metadata_v282(&mut s, mouse);
    exact_v282(&mut s, &f, &steps, 24);
    let before = whole_sql_v282(&f);
    let body = crate::support::overlay_body(
        s.completed_cells_screen().unwrap(),
        "Inbox • focused view ─",
    )
    .unwrap();
    let after = s.completed_cells_end();
    menu(&mut s, mouse, "Add label");
    s.wait_for_command_count(&f, 30);
    failed_open_v290(&mut s, after, Some(&body), true);
    let commands = exact_v282(&mut s, &f, &steps, 30);
    assert_eq!(whole_sql_v282(&f), before);
    let after = s.completed_cells_end();
    s.send_key(b"\x1b[13;5u");
    failed_open_v290(&mut s, after, Some(&body), false);
    assert_eq!(exact_v282(&mut s, &f, &steps, 30), commands);
    assert_eq!(whole_sql_v282(&f), before);
    let after = s.completed_cells_end();
    s.resize(80, 24);
    failed_open_v290(&mut s, after, None, false);
    assert_eq!(whole_sql_v282(&f), before);
    let after = s.completed_cells_end();
    s.resize(120, 40);
    failed_open_v290(&mut s, after, Some(&body), false);
    assert_eq!(fs::read(f.repo().join(".git/config")).unwrap(), config);
    preserve_git(&f, &head, &index, &status);
    s.quit_and_assert_restored_terminal(mouse);
    assert_eq!(exact_v282(&mut s, &f, &steps, 30), commands);
    assert_eq!(whole_sql_v282(&f), before);
    preserve_git(&f, &head, &index, &status);
}
fn next_reservation_v290(mut before: Vec<String>, old: i64) -> Vec<String> {
    // Independently known original parent tuple, not a Source or epoch getter.
    let key = format!(
        "personal-pr-source-v13/{:x}",
        Sha256::digest(
            serde_json::to_vec(&(
                json!({"host":HOST,"login":"user-id:42"}),
                410u64,
                "R_410",
                1007u64,
                PR_NODE,
                7u64
            ))
            .unwrap()
        )
    );
    assert_eq!(
        key,
        "personal-pr-source-v13/97974eedbadbf9ab292d5def48389c54eca7625642b37fe885f571951bf0c1d6"
    );
    let row = |generation| {
        format!(
            "sync_reservations:{:?}",
            [
                rusqlite::types::Value::Text(HOST.into()),
                rusqlite::types::Value::Text("user-id:42".into()),
                rusqlite::types::Value::Text(key.clone()),
                rusqlite::types::Value::Integer(generation),
                rusqlite::types::Value::Integer(1)
            ]
        )
    };
    assert_eq!(
        before
            .iter()
            .filter(|r| r.starts_with("sync_reservations:"))
            .count(),
        1
    );
    let index = before
        .iter()
        .position(|r| r == &row(old))
        .expect("Exact SAME original active Personal read reservation");
    before[index] = row(old.checked_add(1).unwrap());
    before
}
fn visible_forbidden_v290(s: &mut Session, mouse: bool) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        let frame = s
            .completed_cells_screen()
            .expect("Actual completed Label picker");
        let body = crate::support::overlay_body(frame, LABEL_PICKER).expect("SAME owned picker");
        assert!(
            !body.contains("PRIVATE_MEMBER_403_CANARY") && !body.contains("Complete catalogue:")
        );
        if body.contains("(HTTP 403)") && body.contains("no empty catalogue inferred") {
            return;
        }
        let point = crate::support::completed_navigation_controls_v1::owned_action_position(
            frame,
            LABEL_PICKER,
            "[Ctrl-PgDn Detail]",
            &[],
        )
        .expect("Whole actual detail-scroll control, no hidden-model text");
        let after = s.completed_cells_end();
        if mouse {
            s.click(point.0, point.1);
        } else {
            s.send_key(b"\x1b[6;5~");
        }
        loop {
            s.poll_output(Duration::from_millis(1));
            if s.completed_cells_end() > after
                && s.completed_cells_screen().is_some_and(|frame| {
                    crate::support::main_footer_visible(frame)
                        && crate::support::overlay_body(frame, LABEL_PICKER)
                            .is_some_and(|current| current != body)
                })
            {
                break;
            }
            assert!(
                Instant::now() < until,
                "ONE detail scroll needs NEW changed SAME completed picker\n{}",
                s.screen()
            );
        }
        assert!(
            Instant::now() < until,
            "Actual safe403 full fact must become visible by real owned detail scrolling"
        );
    }
}
pub(in crate::pr_actions::pr_metadata::metadata_actions) fn empty_and_forbidden(mouse: bool) {
    let f = fixture();
    let p = original_v282();
    let head = f.git(&["rev-parse", "HEAD"]);
    let index = fs::read(f.repo().join(".git/index")).unwrap();
    let status = f.git(&["status", "--porcelain=v2", "-z"]);
    let config = fs::read(f.repo().join(".git/config")).unwrap();
    let mut first = label_open_v282(&p);
    first[5].stdout = b"[]".to_vec();
    let mut second = label_open_v282(&p);
    second[5].stdout.clear();
    second[5].status = 1;
    second[5].stderr = b"gh: Forbidden PRIVATE_MEMBER_403_CANARY (HTTP 403)\n".to_vec();
    let mut steps = startup_v282(&p);
    steps.extend(first);
    assert_eq!(steps.len(), 35);
    steps.extend(second);
    assert_eq!(steps.len(), 46);
    f.scripted_gh(&steps);
    let mut s = Session::launch(&f, 120, 40);
    enter_metadata_v282(&mut s, mouse);
    exact_v282(&mut s, &f, &steps, 24);
    let before = whole_sql_v282(&f);
    let expected_empty = next_reservation_v290(before, 1);
    let after = s.completed_cells_end();
    menu(&mut s, mouse, "Add label");
    s.wait_for_command_count(&f, 35);
    ready_v283(&mut s, after, LABEL_FORM, SELECT_CONTROLS);
    exact_v282(&mut s, &f, &steps, 35);
    assert_eq!(whole_sql_v282(&f), expected_empty);
    let after = s.completed_cells_end();
    control_v283(
        &mut s,
        mouse,
        LABEL_FORM,
        "[Choose member]",
        SELECT_CONTROLS,
    );
    ready_v283(
        &mut s,
        after,
        LABEL_PICKER,
        &[
            "Complete catalogue:",
            "No observed matching choices",
            "[Enter Choose]",
        ],
    );
    let after = s.completed_cells_end();
    control_v283(
        &mut s,
        mouse,
        LABEL_PICKER,
        "[Enter Choose]",
        PICKER_CONTROLS,
    );
    ready_v283(
        &mut s,
        after,
        LABEL_PICKER,
        &[
            "Choose disabled: no observed matching member",
            "[Enter Choose]",
        ],
    );
    assert_eq!(whole_sql_v282(&f), expected_empty);
    exact_v282(&mut s, &f, &steps, 35);
    control_v283(&mut s, mouse, LABEL_PICKER, "[Esc Back]", PICKER_CONTROLS);
    s.wait_for_overlay_close(LABEL_PICKER);
    s.wait_for_owned_completed_action(LABEL_FORM, "[Esc Close]", &[]);
    let after = s.completed_cells_end();
    control_v283(&mut s, mouse, LABEL_FORM, "[Esc Close]", SELECT_CONTROLS);
    ready_v283(
        &mut s,
        after,
        "Inbox • focused view ─",
        &["Your repository permissions:", "Milestone: Milestone 5"],
    );
    assert_eq!(whole_sql_v282(&f), expected_empty);
    let expected_forbidden = next_reservation_v290(expected_empty, 2);
    let after = s.completed_cells_end();
    menu(&mut s, mouse, "Add label");
    s.wait_for_command_count(&f, 46);
    ready_v283(&mut s, after, LABEL_FORM, SELECT_CONTROLS);
    exact_v282(&mut s, &f, &steps, 46);
    assert_eq!(whole_sql_v282(&f), expected_forbidden);
    let after = s.completed_cells_end();
    control_v283(
        &mut s,
        mouse,
        LABEL_FORM,
        "[Choose member]",
        SELECT_CONTROLS,
    );
    ready_v283(
        &mut s,
        after,
        LABEL_PICKER,
        &["Unavailable:", "[Enter Choose]"],
    );
    visible_forbidden_v290(&mut s, mouse);
    let body =
        crate::support::overlay_body(s.completed_cells_screen().unwrap(), LABEL_PICKER).unwrap();
    assert!(
        !body.contains("Complete catalogue:")
            && !body.contains("PRIVATE_MEMBER_403_CANARY")
            && !body.contains("[Confirm action]")
            && !body.contains("[Clear milestone]")
            && !body.contains("Clear all labels")
    );
    let after = s.completed_cells_end();
    control_v283(
        &mut s,
        mouse,
        LABEL_PICKER,
        "[Enter Choose]",
        PICKER_CONTROLS,
    );
    ready_v283(
        &mut s,
        after,
        LABEL_PICKER,
        &[
            "Choose disabled: no observed matching member",
            "[Enter Choose]",
        ],
    );
    let commands = exact_v282(&mut s, &f, &steps, 46);
    assert_eq!(whole_sql_v282(&f), expected_forbidden);
    control_v283(&mut s, mouse, LABEL_PICKER, "[Esc Back]", PICKER_CONTROLS);
    s.wait_for_overlay_close(LABEL_PICKER);
    s.wait_for_owned_completed_action(LABEL_FORM, "[Esc Close]", &[]);
    let after = s.completed_cells_end();
    control_v283(&mut s, mouse, LABEL_FORM, "[Esc Close]", SELECT_CONTROLS);
    ready_v283(
        &mut s,
        after,
        "Inbox • focused view ─",
        &["Your repository permissions:", "Milestone: Milestone 5"],
    );
    let after = s.completed_cells_end();
    s.resize(80, 24);
    ready_v283(
        &mut s,
        after,
        "Inbox • focused view ─",
        &["example/repo#7", "Your repository permissions:"],
    );
    assert_eq!(whole_sql_v282(&f), expected_forbidden);
    assert!(!s.screen().contains("PRIVATE_MEMBER_403_CANARY"));
    assert_eq!(fs::read(f.repo().join(".git/config")).unwrap(), config);
    preserve_git(&f, &head, &index, &status);
    s.quit_and_assert_restored_terminal(mouse);
    assert_eq!(exact_v282(&mut s, &f, &steps, 46), commands);
    assert_eq!(whole_sql_v282(&f), expected_forbidden);
    preserve_git(&f, &head, &index, &status);
}
pub(in crate::pr_actions::pr_metadata::metadata_actions) fn partial(mouse: bool) {
    let f = fixture();
    let p = original_v282();
    let head = f.git(&["rev-parse", "HEAD"]);
    let index = fs::read(f.repo().join(".git/index")).unwrap();
    let status = f.git(&["status", "--porcelain=v2", "-z"]);
    let config = fs::read(f.repo().join(".git/config")).unwrap();
    let mut candidate = values::candidate(PrMetadataCommand::AddLabel);
    candidate["archived_at"] = Value::Null;
    candidate["archived_by"] = Value::Null;
    let mut rows = vec![
        json!({"id":null,"node_id":null,"name":"unverified","archived_at":null,"archived_by":null}),
        candidate,
    ];
    // Preserve the actual1000 raw-entry/ten-full-page boundary, not1000
    // fully identified options: that old five-field forest alone needs over
    //9MiB in the unchanged monotonic4MiB Metadata decoded/model accounting.
    // Unknown null entries stay unknown/disabled; positions never become IDs.
    rows.resize(1000, Value::Null);
    assert_eq!(rows.len(), 1000);
    assert_eq!(rows.iter().filter(|row| row.is_null()).count(), 998);
    assert_eq!(rows[0]["name"], "unverified");
    assert!(rows[0]["id"].is_null() && rows[0]["node_id"].is_null());
    assert!(rows[0].get("archived_at").is_some_and(Value::is_null));
    assert!(rows[0].get("archived_by").is_some_and(Value::is_null));
    assert_eq!(
        rows.iter()
            .filter_map(|row| row["id"].as_u64())
            .collect::<Vec<_>>(),
        [81]
    );
    let pages = rows
        .chunks(100)
        .enumerate()
        .map(|(n, rows)| {
            values::get(
                &format!("repos/example/repo/labels?per_page=100&page={}", n + 1),
                json!(rows),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(pages.len(), 10);
    let mut loading = label_open_v282(&p);
    loading.splice(5..6, pages);
    assert_eq!(
        loading.len(),
        20,
        "Core6+rawPR/issue/10actualpages/GQL/closingraw"
    );
    let mut steps = startup_v282(&p);
    steps.extend(loading);
    let opened = steps.len();
    steps.extend(label_open_v282(&p));
    let previewed = steps.len();
    assert_eq!(
        (opened, previewed),
        (44, 55),
        "Fresh Preview is an independent actual eleven-child phase"
    );
    f.scripted_gh(&steps);
    let mut s = Session::launch(&f, 120, 40);
    enter_metadata_v282(&mut s, mouse);
    let after = s.completed_cells_end();
    menu(&mut s, mouse, "Add label");
    s.wait_for_command_count(&f, opened);
    ready_v283(
        &mut s,
        after,
        LABEL_FORM,
        &[
            "Selected actual member:",
            "[Choose member]",
            "[Preview]",
            "[Esc Close]",
        ],
    );
    exact_v282(&mut s, &f, &steps, opened);
    let before = whole_sql_v282(&f);
    let after = s.completed_cells_end();
    control_v283(
        &mut s,
        mouse,
        LABEL_FORM,
        "[Choose member]",
        SELECT_CONTROLS,
    );
    ready_v283(
        &mut s,
        after,
        LABEL_PICKER,
        &[
            "Partial catalogue: observed choices only",
            "unverified",
            "[Enter Choose]",
            "[Esc Back]",
        ],
    );
    let after = s.completed_cells_end();
    control_v283(
        &mut s,
        mouse,
        LABEL_PICKER,
        "[Enter Choose]",
        PICKER_CONTROLS,
    );
    ready_v283(
        &mut s,
        after,
        LABEL_PICKER,
        &[
            "Label name/positive source ID is unavailable",
            "! disabled · Label: unverified",
            "[Enter Choose]",
        ],
    );
    exact_v282(&mut s, &f, &steps, opened);
    assert_eq!(whole_sql_v282(&f), before);
    let after = s.completed_cells_end();
    if mouse {
        control_v283(
            &mut s,
            true,
            LABEL_PICKER,
            "[Ctrl-U Clear]",
            PICKER_CONTROLS,
        );
    } else {
        // Clear is an advertised keyboard shortcut, not a Tab focus stop.
        crate::support::completed_navigation_controls_v1::owned_action_position(
            s.completed_cells_screen().unwrap(),
            LABEL_PICKER,
            "[Ctrl-U Clear]",
            &[],
        )
        .expect("Whole actual owning picker advertises Ctrl-U Clear");
        s.send_key(b"\x15");
    }
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > after
            && s.completed_cells_screen().is_some_and(|frame| {
                crate::support::main_footer_visible(frame)
                    && crate::support::overlay_body(
                        frame,
                        &format!("{LABEL_PICKER} · Filter focused"),
                    )
                    .is_some_and(|body| {
                        body.lines().any(|row| row.trim() == "Filter focused:")
                            && PICKER_CONTROLS.iter().all(|label| body.contains(label))
                            && !body.contains("[Esc Cancel]")
                    })
            })
        {
            break;
        }
        assert!(
            Instant::now() < until,
            "ONE actual Clear requires NEW same-owned empty Filter-focused picker\n{}",
            s.screen()
        );
    }
    exact_v282(&mut s, &f, &steps, opened);
    assert_eq!(whole_sql_v282(&f), before);
    let after = s.completed_cells_end();
    s.paste("new 🐛");
    ready_v283(
        &mut s,
        after,
        LABEL_PICKER,
        &[
            "Filter focused: new 🐛",
            "◆ eligible · Label: new 🐛",
            "[Enter Choose]",
        ],
    );
    let after = s.completed_cells_end();
    if mouse {
        let frame = s.completed_cells_screen().unwrap();
        let (x, y) = crate::support::completed_navigation_controls_v1::owned_action_position(
            frame,
            LABEL_PICKER,
            "◆ eligible · Label: new 🐛",
            &[],
        )
        .expect("Actual canonical eligible member row");
        s.click(x, y);
    } else {
        s.send_key(b"\t");
    }
    ready_v283(
        &mut s,
        after,
        &format!("{LABEL_PICKER} · Rows focused"),
        PICKER_CONTROLS,
    );
    control_v283(
        &mut s,
        mouse,
        LABEL_PICKER,
        "[Enter Choose]",
        PICKER_CONTROLS,
    );
    s.wait_for_overlay_close(LABEL_PICKER);
    s.wait_for_owned_completed_action(LABEL_FORM, "[Preview]", &[]);
    let value = "new 🐛 • actual #81 • L_81";
    assert!(
        crate::support::overlay_body(s.completed_cells_screen().unwrap(), LABEL_FORM)
            .unwrap()
            .contains(value)
    );
    exact_v282(&mut s, &f, &steps, opened);
    assert_eq!(whole_sql_v282(&f), before);
    let key = selected_draft_key_v283();
    assert_eq!(saved_v283(&f, &key), None);
    let stable = unrelated_sql_v283(&f, &key);
    let after = s.completed_cells_end();
    control_v283(&mut s, mouse, LABEL_FORM, "[Preview]", SELECT_CONTROLS);
    s.wait_for_command_count(&f, previewed);
    ready_v283(
        &mut s,
        after,
        LABEL_FORM,
        &[
            "READ-ONLY PREVIEW",
            "Exact effect: Add label",
            "L_81",
            "[Confirm action]",
            "[Keep draft / Close]",
        ],
    );
    let expected = json!({"version":17,"values":{"body":value},"publication":null});
    s.try_wait_for_condition(
        |_| {
            saved_v283(&f, &key)
                .is_some_and(|body| serde_json::from_str::<Value>(&body).unwrap() == expected)
        },
        "Known exact v17 selected member autosave settles without publication",
        Duration::from_secs(8),
    )
    .unwrap();
    assert_eq!(unrelated_sql_v283(&f, &key), stable);
    exact_v282(&mut s, &f, &steps, previewed);
    let after = s.completed_cells_end();
    control_v283(
        &mut s,
        mouse,
        LABEL_FORM,
        "[Keep draft / Close]",
        PREVIEW_CONTROLS,
    );
    ready_v283(
        &mut s,
        after,
        "Inbox • focused view ─",
        &["Your repository permissions:", "Milestone: Milestone 5"],
    );
    assert_eq!(unrelated_sql_v283(&f, &key), stable);
    assert_eq!(
        serde_json::from_str::<Value>(&saved_v283(&f, &key).unwrap()).unwrap(),
        expected
    );
    assert_eq!(fs::read(f.repo().join(".git/config")).unwrap(), config);
    preserve_git(&f, &head, &index, &status);
    s.quit_and_assert_restored_terminal(mouse);
    exact_v282(&mut s, &f, &steps, previewed);
    assert_eq!(unrelated_sql_v283(&f, &key), stable);
    preserve_git(&f, &head, &index, &status);
}
