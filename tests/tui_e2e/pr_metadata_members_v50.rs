//! Actual binary ALL10 Members. Strict fixtures are expected data only; no
//! original, permission, receipt or generation is fabricated by this driver.
use super::*;
use ghtui::pr_metadata_actions::PrMetadataCommand as Cmd;
#[path = "pr_issue_links_v102.rs"]
mod issue_links_v102;
#[path = "pr_metadata_members_recovery_v50.rs"]
mod recovery_v50;

fn independent(f: &Fixture) -> Vec<String> {
    // Parent's foreign-draft observer includes every non-Text draft. Exclude
    // ONLY this current account's named Members family, whose exact envelope
    // and retention/deletion are independently asserted in each journey.
    super::independent(f)
        .into_iter()
        .filter(|encoded| {
            let row: Value = serde_json::from_str(encoded).unwrap();
            !(row.as_array().is_some_and(|row| row.len() == 4)
                && row[0] == HOST
                && row[1] == "user-id:42"
                && row[2]
                    .as_str()
                    .is_some_and(|resource| resource.starts_with("metadata-members-v48/")))
        })
        .collect()
}
fn finish(
    mut s: Session,
    f: &Fixture,
    mouse: bool,
    steps: &[GhStep],
    git: &GitState,
    config: &[u8],
    stable: &[String],
) {
    assert_eq!(independent(f), stable);
    assert_eq!(&git_state(f), git);
    assert_eq!(
        fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    exact(&s, f, steps);
    assert_read_only_git(f);
    s.quit_and_assert_restored_terminal(mouse);
}
fn field(command: Cmd) -> &'static str {
    match command {
        Cmd::AddLabel | Cmd::RemoveLabel => "labels",
        Cmd::AddAssignee | Cmd::RemoveAssignee => "assignees",
        Cmd::AddReviewer | Cmd::RemoveReviewer => "reviewers",
        Cmd::AddTeam | Cmd::RemoveTeam => "teams",
        _ => "milestone",
    }
}
fn adds(command: Cmd) -> bool {
    matches!(
        command,
        Cmd::AddLabel | Cmd::AddAssignee | Cmd::AddReviewer | Cmd::AddTeam | Cmd::SetMilestone
    )
}
fn repo() -> Value {
    let mut raw = repository(0);
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
    let mut raw = parent_resource();
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
    issue["pull_request"] = json!({"url":format!("https://github.invalid/api/v3/repos/{FIRST}/pulls/7"),"html_url":format!("https://github.invalid/{FIRST}/pull/7")});
    issue
}
fn candidate(command: Cmd) -> Value {
    match field(command) {
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
    let selected=match field(command){
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
    json!({"data":{"node":{"__typename":"PullRequest","id":"GENUINE_CANONICAL_PR_NODE_1007","fullDatabaseId":"1007","number":7,
    "headRefOid":"a".repeat(40),"baseRefOid":"b".repeat(40),"headRefName":"feature/scoped-open","baseRefName":"main","state":"OPEN","isDraft":false,
    "repository":{"id":"REPO_catalogue_10000","nameWithOwner":FIRST},
    "labels":connection(vec![json!({"id":"LABEL_10","name":"kept"})]),
    "assignees":connection(vec![json!({"id":"USER_20","login":"current-assignee"})]),
    "milestone":{"id":"MILESTONE_50","number":1,"title":"Current milestone"},
    "reviewRequests":connection(vec![json!({"id":"REQUEST_USER_30","requestedReviewer":{"__typename":"User","id":"USER_30","login":"current-reviewer"}}),json!({"id":"REQUEST_TEAM_40","requestedReviewer":{"__typename":"Team","id":"TEAM_40","slug":"current-team","organization":{"login":"example"}}})])}}})
}
fn graphql(command: Cmd, value: Value, selected_team: bool) -> GhStep {
    let mut step = get("graphql", value);
    *step.args.iter_mut().find(|s| s.as_str() == "GET").unwrap() = "POST".into();
    step.args.extend(["--input".into(), "-".into()]);
    let mut variables = json!({"id":"GENUINE_CANONICAL_PR_NODE_1007"});
    if selected_team {
        variables["candidate"] = json!("TEAM_41");
    }
    step.stdin =
        serde_json::to_vec(&json!({"query":query(command,selected_team),"variables":variables}))
            .unwrap();
    step
}
fn phase(command: Cmd, raw: Value, choice: Value, gql: Value) -> Vec<GhStep> {
    let endpoint = match field(command) {
        "labels" => "labels",
        "assignees" => "assignees",
        "reviewers" => "collaborators",
        "teams" => "teams",
        "milestone" => "milestones?state=open",
        _ => unreachable!(),
    };
    let separator = if endpoint.contains('?') { '&' } else { '?' };
    let mut calls = principal(42);
    calls.push(get(&format!("repos/{FIRST}"), repo()));
    calls.push(get(&format!("repos/{FIRST}/pulls/7"), raw.clone()));
    calls.push(get(&format!("repos/{FIRST}/issues/7"), issue(&raw)));
    calls.push(get(
        &format!("repos/{FIRST}/{endpoint}{separator}per_page=100&page=1"),
        json!([choice]),
    ));
    calls.push(graphql(command, gql, false));
    calls.push(get(&format!("repos/{FIRST}/pulls/7"), raw));
    calls.push(get(&format!("repos/{FIRST}"), repo()));
    calls.extend(principal(42));
    assert_eq!(calls.len(), 11);
    calls
}
fn after(command: Cmd) -> Value {
    let mut next = raw();
    let key = match field(command) {
        "reviewers" => "requested_reviewers",
        "teams" => "requested_teams",
        other => other,
    };
    match command {
        Cmd::ClearMilestone => next[key] = Value::Null,
        Cmd::SetMilestone => next[key] = candidate(command),
        _ if adds(command) => next[key].as_array_mut().unwrap().push(candidate(command)),
        _ => next[key] = json!([]),
    }
    next
}
fn current_after(command: Cmd) -> Value {
    let mut gql = current();
    let node = &mut gql["data"]["node"];
    match command {
        Cmd::AddLabel => {
            node["labels"] = connection(vec![
                json!({"id":"LABEL_10","name":"kept"}),
                json!({"id":"LABEL_11","name":"new-label"}),
            ])
        }
        Cmd::RemoveLabel => node["labels"] = connection(vec![]),
        Cmd::AddAssignee => {
            node["assignees"] = connection(vec![
                json!({"id":"USER_20","login":"current-assignee"}),
                json!({"id":"USER_21","login":"new-assignee"}),
            ])
        }
        Cmd::RemoveAssignee => node["assignees"] = connection(vec![]),
        Cmd::SetMilestone => {
            node["milestone"] = json!({"id":"MILESTONE_51","number":2,"title":"Next milestone"})
        }
        Cmd::ClearMilestone => node["milestone"] = Value::Null,
        _ => {
            let mut rows = vec![];
            if command != Cmd::RemoveReviewer {
                rows.push(json!({"id":"REQUEST_USER_30","requestedReviewer":{"__typename":"User","id":"USER_30","login":"current-reviewer"}}));
            }
            if command != Cmd::RemoveTeam {
                rows.push(json!({"id":"REQUEST_TEAM_40","requestedReviewer":{"__typename":"Team","id":"TEAM_40","slug":"current-team","organization":{"login":"example"}}}));
            }
            if command == Cmd::AddReviewer {
                rows.push(json!({"id":"REQUEST_USER_31","requestedReviewer":{"__typename":"User","id":"USER_31","login":"new-reviewer"}}));
            }
            if command == Cmd::AddTeam {
                rows.push(json!({"id":"REQUEST_TEAM_41","requestedReviewer":{"__typename":"Team","id":"TEAM_41","slug":"new-team","organization":{"login":"example"}}}));
            }
            node["reviewRequests"] = connection(rows);
        }
    }
    if command == Cmd::AddTeam {
        gql["data"]["candidate"] = json!({"__typename":"Team","id":"TEAM_41","slug":"new-team","organization":{"login":"example"}});
    }
    gql
}
fn current_before(command: Cmd) -> Value {
    let mut gql = current();
    if command == Cmd::AddTeam {
        gql["data"]["candidate"] = json!({"__typename":"Team","id":"TEAM_41","slug":"new-team","organization":{"login":"example"}});
    }
    gql
}
fn selected_phase(command: Cmd) -> Vec<GhStep> {
    let mut calls = phase(command, raw(), candidate(command), current_before(command));
    calls[6] = graphql(command, current_before(command), command == Cmd::AddTeam);
    calls
}
fn mutation_input(command: Cmd) -> Value {
    match command {
        Cmd::AddLabel => {
            json!({"pullRequestId":"GENUINE_CANONICAL_PR_NODE_1007","labelIds":["LABEL_10","LABEL_11"]})
        }
        Cmd::RemoveLabel => json!({"pullRequestId":"GENUINE_CANONICAL_PR_NODE_1007","labelIds":[]}),
        Cmd::AddAssignee => {
            json!({"pullRequestId":"GENUINE_CANONICAL_PR_NODE_1007","assigneeIds":["USER_20","USER_21"]})
        }
        Cmd::RemoveAssignee => {
            json!({"pullRequestId":"GENUINE_CANONICAL_PR_NODE_1007","assigneeIds":[]})
        }
        Cmd::AddReviewer => {
            json!({"pullRequestId":"GENUINE_CANONICAL_PR_NODE_1007","union":true,"userIds":["USER_31"]})
        }
        Cmd::AddTeam => {
            json!({"pullRequestId":"GENUINE_CANONICAL_PR_NODE_1007","union":true,"teamIds":["TEAM_41"]})
        }
        Cmd::RemoveReviewer => {
            json!({"pullRequestId":"GENUINE_CANONICAL_PR_NODE_1007","union":false,"userIds":[],"teamIds":["TEAM_40"]})
        }
        Cmd::RemoveTeam => {
            json!({"pullRequestId":"GENUINE_CANONICAL_PR_NODE_1007","union":false,"userIds":["USER_30"],"teamIds":[]})
        }
        Cmd::SetMilestone => {
            json!({"pullRequestId":"GENUINE_CANONICAL_PR_NODE_1007","milestoneId":"MILESTONE_51"})
        }
        Cmd::ClearMilestone => {
            json!({"pullRequestId":"GENUINE_CANONICAL_PR_NODE_1007","milestoneId":null})
        }
    }
}
fn post(command: Cmd) -> Vec<GhStep> {
    let reviews = matches!(field(command), "reviewers" | "teams");
    let query = if reviews {
        ghtui::providers::gh::PR_METADATA_REQUEST_REVIEWS_MUTATION
    } else {
        ghtui::providers::gh::PR_METADATA_UPDATE_MUTATION
    };
    let mut mutation = graphql(
        command,
        if reviews {
            json!({"data":{"requestReviews":{"pullRequest":{"id":"GENUINE_CANONICAL_PR_NODE_1007"}}}})
        } else {
            json!({"data":{"updatePullRequest":{"pullRequest":{"id":"GENUINE_CANONICAL_PR_NODE_1007"}}}})
        },
        false,
    );
    mutation.stdin =
        serde_json::to_vec(&json!({"query":query,"variables":{"input":mutation_input(command)}}))
            .unwrap();
    let after = after(command);
    let mut calls = vec![
        mutation,
        graphql(command, current_after(command), command == Cmd::AddTeam),
        get(&format!("repos/{FIRST}"), repo()),
        get(&format!("repos/{FIRST}/pulls/7"), after.clone()),
        get(&format!("repos/{FIRST}/issues/7"), issue(&after)),
        get("user", json!({"id":42,"login":"octocat"})),
        get("user", json!({"id":42,"login":"octocat"})),
        get(&format!("repos/{FIRST}"), repo()),
        get(&format!("repos/{FIRST}/pulls/7"), after.clone()),
        get(&format!("repos/{FIRST}/issues/7"), issue(&after)),
        graphql(command, current_after(command), command == Cmd::AddTeam),
    ];
    for (endpoint, rows) in [
        ("issues/7/comments", json!([])),
        ("pulls/7/commits", json!([])),
        (
            "pulls/7/files",
            json!([{"filename":"src/view.rs","status":"modified","additions":1,"deletions":1,"patch":"@@ -1 +1 @@\n-old\n+new\n"}]),
        ),
        ("pulls/7/reviews", json!([])),
        ("issues/7/timeline", json!([])),
    ] {
        let mut child = get(
            &format!("repos/{FIRST}/{endpoint}?per_page=100"),
            json!([rows]),
        );
        child.args.extend(["--paginate".into(), "--slurp".into()]);
        calls.push(child);
    }
    calls.push(get(&format!("repos/{FIRST}/pulls/7"), after));
    calls.push(get("user", json!({"id":42,"login":"octocat"})));
    assert_eq!(calls.len(), 18);
    calls
}

const PICKER_CONTROLS: &[&str] = &[
    "[Enter Choose]",
    "[Esc Back]",
    "[Ctrl-U Clear]",
    "[PgUp]",
    "[PgDn]",
];
const SELECT_CONTROLS: &[&str] = &["[Choose member]", "[Preview]", "[Esc Close]"];
const CLEAR_CONTROLS: &[&str] = &["[Preview]", "[Esc Close]"];
const PREVIEW_CONTROLS: &[&str] = &["[Preview]", "[Confirm action]", "[Keep draft / Close]"];
fn control(s: &mut Session, mouse: bool, title: &str, label: &str, controls: &[&str]) {
    const ERROR: &str = "Choose an actual indexed member before Preview";
    const ERROR_CONTROLS: &[&str] = &[
        "[Choose member]",
        "[Preview]",
        "[Esc Close]",
        "[F7 Error↑]",
        "[F8 Error↓]",
    ];
    let missing_selection = !mouse
        && label == "[Choose member]"
        && controls == SELECT_CONTROLS
        && s.completed_cells_screen().is_some_and(|frame| {
            crate::support::main_footer_visible(frame)
                && crate::support::overlay_body(frame, title).is_some_and(|body| {
                    body.contains(ERROR)
                        && ERROR_CONTROLS.iter().all(|control| body.contains(control))
                })
        });
    if !missing_selection {
        return super::control(s, mouse, title, label, controls);
    }
    // Preview's local refusal keeps its actual focus and adds TWO real Error
    // controls. Reverse toward Choose instead of crossing the editable field;
    // each key must cause a NEW same-topmost-owner visible focus change.
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        let observe = |frame: &vt100::Screen| {
            (crate::support::main_footer_visible(frame)
                && crate::support::overlay_body(frame, title).is_some_and(|body| {
                    body.contains(ERROR)
                        && ERROR_CONTROLS.iter().all(|control| body.contains(control))
                }))
            .then(|| {
                crate::support::completed_navigation_controls_v1::owned_focus_v1(
                    frame,
                    title,
                    label,
                    FocusAttributeV1::Inverse,
                    ERROR_CONTROLS,
                    &[],
                )
            })
            .flatten()
        };
        let Some(previous) = s.completed_cells_screen().and_then(observe) else {
            assert!(
                Instant::now() < until,
                "Exact owned Members missing-selection focus/complete controls absent\n{}",
                s.screen()
            );
            continue;
        };
        if previous.target_focused {
            s.send_key(b"\r");
            return;
        }
        let after = s.completed_cells_end();
        s.send_key(b"\x1b[Z");
        loop {
            s.poll_output(Duration::from_millis(1));
            if s.completed_cells_end() > after
                && s.completed_cells_screen()
                    .and_then(observe)
                    .is_some_and(|current| current.acknowledges_change_from(&previous))
            {
                break;
            }
            assert!(
                Instant::now() < until,
                "ONE BackTab must change exact owned Members error focus before next input\n{}",
                s.screen()
            );
        }
    }
}
fn members_title(command: Cmd) -> String {
    format!("{} • {FIRST}#7", command.label())
}
fn picker_title(command: Cmd) -> &'static str {
    match (adds(command), field(command)) {
        (true, "labels") => "PR metadata choices · Label",
        (false, "labels") => "Current PR metadata members · Label",
        (true, "assignees") => "PR metadata choices · Assignee",
        (false, "assignees") => "Current PR metadata members · Assignee",
        (true, "reviewers") => "PR metadata choices · Reviewer",
        (false, "reviewers") => "Current PR metadata members · Reviewer",
        (true, "teams") => "PR metadata choices · Reviewer team",
        (false, "teams") => "Current PR metadata members · Reviewer team",
        _ => "PR metadata choices · Milestone",
    }
}
fn picker_control(s: &mut Session, mouse: bool, command: Cmd, label: &str) {
    if mouse {
        return control(s, true, picker_title(command), label, PICKER_CONTROLS);
    }
    // Query -> Rows is a real focus change without any footer inverse.
    // Choose AND Back must acknowledge that SAME completed owned title
    // before the next Tab. Short "Filter focused" is also a query caption.
    let notices = [
        format!("{} · Filter focused", picker_title(command)),
        format!("{} · Rows focused", picker_title(command)),
    ];
    let notices = notices.each_ref().map(String::as_str);
    s.focus_completed_owned_action_v1(
        picker_title(command),
        label,
        FocusAttributeV1::Inverse,
        PICKER_CONTROLS,
        &notices,
    );
    s.send_key(b"\r");
}
fn chosen_label(command: Cmd) -> &'static str {
    match command {
        Cmd::AddLabel => "new-label",
        Cmd::RemoveLabel => "kept",
        Cmd::AddAssignee => "new-assignee",
        Cmd::RemoveAssignee => "current-assignee",
        Cmd::AddReviewer => "new-reviewer",
        Cmd::RemoveReviewer => "current-reviewer",
        Cmd::AddTeam => "new-team",
        Cmd::RemoveTeam => "current-team",
        Cmd::SetMilestone => "Next milestone",
        Cmd::ClearMilestone => "Current milestone",
    }
}
fn selected_value(command: Cmd) -> String {
    let (id, node) = match command {
        Cmd::AddLabel => (11, "LABEL_11"),
        Cmd::RemoveLabel => (10, "LABEL_10"),
        Cmd::AddAssignee => (21, "USER_21"),
        Cmd::RemoveAssignee => (20, "USER_20"),
        Cmd::AddReviewer => (31, "USER_31"),
        Cmd::RemoveReviewer => (30, "USER_30"),
        Cmd::AddTeam => (41, "TEAM_41"),
        Cmd::RemoveTeam => (40, "TEAM_40"),
        Cmd::SetMilestone => (51, "MILESTONE_51"),
        Cmd::ClearMilestone => return "Clear the actual current milestone".into(),
    };
    format!("{} • actual #{id} • {node}", chosen_label(command))
}
fn selection_controls(command: Cmd) -> &'static [&'static str] {
    if command == Cmd::ClearMilestone {
        CLEAR_CONTROLS
    } else {
        SELECT_CONTROLS
    }
}
fn begin(
    steps: &[GhStep],
    personal: bool,
    mouse: bool,
    initial: usize,
) -> (Fixture, Session, GitState, Vec<u8>, Vec<String>) {
    let f = fixture(steps);
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
    if personal {
        let after = s.completed_cursorless_end();
        choose(&mut s, mouse, "[c Conversation]", b"c");
        s.wait_for_new_cursorless_condition(
            after,
            |screen| {
                crate::support::main_footer_visible(screen)
                    && screen.contents().contains("Conversation")
            },
            "Actual original Personal Conversation tab",
            Duration::from_secs(8),
        );
    }
    let stable = independent(&f);
    (f, s, git, config, stable)
}
fn open_members(s: &mut Session, f: &Fixture, mouse: bool, command: Cmd, end: usize) {
    let after = s.completed_cells_end();
    crate::pr_actions::menu(s, mouse, command.label());
    s.wait_for_command_count(f, end);
    form_frame(
        s,
        after,
        &members_title(command),
        selection_controls(command),
    );
    let body =
        crate::support::overlay_body(s.completed_cells_screen().unwrap(), &members_title(command))
            .unwrap();
    // The initial intent paints actual identity and the selected-member field,
    // not the prepared plan lines (those are rendered only in Preview).
    // The exact command/PR title and real controls were checked above; numeric
    // repository identity remains independently checked by the strict provider
    // protocol and the typed receipt, not invented as currently visible text.
    assert!(body.contains("Requester octocat • account 42 • original PR metadata • no resend"));
    assert!(body.contains("Original author @actual-pr-author • account #61 (not requester)"));
    assert!(body.contains("PR INTENT • Preview first, then explicitly confirm"));
    assert!(body.contains("Selected actual member:"));
    assert!(!body.contains("[Confirm action]"));
    assert!(!body.contains("Some(42)"));
}
fn select(s: &mut Session, f: &Fixture, mouse: bool, command: Cmd, expected: &[GhStep]) {
    select_with_expected(s, f, mouse, command, expected, 0, 0)
}
fn select_with_expected(
    s: &mut Session,
    f: &Fixture,
    mouse: bool,
    command: Cmd,
    expected: &[GhStep],
    attempts: usize,
    drafts: usize,
) {
    if command != Cmd::ClearMilestone {
        let after = s.completed_cells_end();
        control(
            s,
            mouse,
            &members_title(command),
            "[Choose member]",
            selection_controls(command),
        );
        form_frame(
            s,
            after,
            picker_title(command),
            &[chosen_label(command), "[Enter Choose]", "[Esc Back]"],
        );
        if mouse {
            s.click_visible_action(chosen_label(command));
        }
        let after = s.completed_cells_end();
        picker_control(s, mouse, command, "[Enter Choose]");
        form_frame(
            s,
            after,
            &members_title(command),
            selection_controls(command),
        );
    }
    exact(s, f, expected);
    let db = super::raw(f);
    assert_eq!(
        db.query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
            .get::<_, usize>(0))
            .unwrap(),
        attempts
    );
    assert_eq!(db.query_row("SELECT count(*) FROM drafts WHERE resource LIKE 'metadata-members-v48/%'",[],|r|r.get::<_,usize>(0)).unwrap(),drafts,"Picker selection, including Clear, never overwrites a protected draft or pretends Saved ACK");
}
fn prepare(s: &mut Session, f: &Fixture, mouse: bool, command: Cmd, end: usize) {
    prepare_with_expected(s, f, mouse, command, end, 0)
}
fn prepare_with_expected(
    s: &mut Session,
    f: &Fixture,
    mouse: bool,
    command: Cmd,
    end: usize,
    attempts: usize,
) {
    let after = s.completed_cells_end();
    control(
        s,
        mouse,
        &members_title(command),
        "[Preview]",
        selection_controls(command),
    );
    s.wait_for_command_count(f, end);
    form_frame(
        s,
        after,
        &members_title(command),
        &[
            "READ-ONLY PREVIEW",
            "[Confirm action]",
            "[Keep draft / Close]",
        ],
    );
    let after = s.completed_cells_end();
    s.paste("POISON_MEMBERS_PREVIEW_BODY");
    form_frame(s, after, &members_title(command), PREVIEW_CONTROLS);
    assert!(!s.screen().contains("POISON_MEMBERS_PREVIEW_BODY"));
    // A rendered Preview is not a Saved ACK. Observe the SAME current Form's
    // real acknowledgement and exact persisted initial envelope before any
    // Confirm or full-SQL baseline, including bodyless Clear after prior work.
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        let acknowledged = s.completed_cells_screen().is_some_and(|frame| {
            crate::support::main_footer_visible(frame)
                && frame
                    .contents()
                    .contains("Draft saved locally • nothing published")
                && crate::support::overlay_body(frame, &members_title(command)).is_some_and(
                    |body| {
                        PREVIEW_CONTROLS
                            .iter()
                            .all(|control| body.contains(control))
                    },
                )
        });
        let saved: usize = super::raw(f).query_row("SELECT count(*) FROM drafts WHERE resource LIKE 'metadata-members-v48/%' AND json_extract(body,'$.version')=17 AND json_type(body,'$.publication')='null' AND json_extract(body,'$.values.body')=?1",[selected_value(command)],|row|row.get(0)).unwrap();
        if acknowledged && saved == 1 {
            break;
        }
        assert!(
            Instant::now() < until,
            "Exact accepted initial Members save must settle before Confirm/baseline\n{}",
            s.screen()
        );
    }
    assert_eq!(
        super::raw(f)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                .get::<_, usize>(0))
            .unwrap(),
        attempts
    );
    s.capture_completed_terminal_v1("metadata-members-real-selected-preview");
}
fn resize_members(s: &mut Session, command: Cmd, width: u16, height: u16, controls: &[&str]) {
    let title = members_title(command);
    let old = s.completed_cells_screen().map(|screen| screen.size());
    let after = s.completed_cells_end();
    s.resize(width, height);
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if (old == Some((height, width)) || s.completed_cells_end() > after)
            && s.completed_cells_screen().is_some_and(|screen| {
                screen.size() == (height, width)
                    && crate::support::main_footer_visible(screen)
                    && crate::support::overlay_body(screen, &title).is_some_and(|body| {
                        body.lines().any(|row| {
                            let mut rest = row;
                            controls.iter().all(|label| {
                                if let Some(offset) = rest.find(label) {
                                    rest = &rest[offset + label.len()..];
                                    true
                                } else {
                                    false
                                }
                            })
                        })
                    })
            })
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "NEW exact Members owned control row at {width}x{height}\n{}",
            s.screen()
        );
    }
}
fn receipt(f: &Fixture, command: Cmd) {
    let db = super::raw(f);
    let(purpose,state,head,effect,key):(String,String,String,String,String)=db.query_row("SELECT command,state,observed_head,confirmed_json,selected_resource FROM pr_write_attempts",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).unwrap();
    assert_eq!(
        (purpose, state, head),
        (command.key().into(), "confirmed".into(), "a".repeat(40))
    );
    let effect: ghtui::pr_write_journal::PrConfirmedEffect = serde_json::from_str(&effect).unwrap();
    let typed = ghtui::pr_metadata_actions::PrMetadataReceipt::decode(&effect).unwrap();
    assert_eq!(typed.command(), command);
    assert_eq!(typed.repository_id(), 10000);
    assert_eq!(
        typed.selected_id(),
        Some(match command {
            Cmd::AddLabel => 11,
            Cmd::RemoveLabel => 10,
            Cmd::AddAssignee => 21,
            Cmd::RemoveAssignee => 20,
            Cmd::AddReviewer => 31,
            Cmd::RemoveReviewer => 30,
            Cmd::AddTeam => 41,
            Cmd::RemoveTeam => 40,
            Cmd::SetMilestone => 51,
            Cmd::ClearMilestone => 50,
        })
    );
    let expected_preserved: &[u64] = match field(command) {
        "reviewers" => &[40],
        "teams" => &[30],
        _ => &[],
    };
    assert_eq!(
        typed.preserved_review_ids(),
        expected_preserved,
        "Opposite User/Team membership is actual typed receipt evidence"
    );
    assert_eq!(
        effect.node_id.as_deref(),
        Some("GENUINE_CANONICAL_PR_NODE_1007")
    );
    assert!(key
        .strip_prefix("metadata-members-v48/")
        .is_some_and(|s| s.len() == 64));
    let(version,base,action,ack):(u8,String,String,Option<i64>)=db.query_row("SELECT json_extract(origin_json,'$.seed.version'),base_key_v48,action_tag_v48,parent_ack_attempt_v48 FROM pr_write_metadata_origins_v25",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap();
    assert_eq!(version, 48);
    assert_eq!(action, command.key());
    assert!(ack.is_none());
    assert!(base
        .strip_prefix("metadata-members-base-v48/")
        .is_some_and(|s| s.len() == 64));
    assert_eq!(
        db.query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
            .get::<_, usize>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM drafts WHERE resource LIKE 'metadata-members-v48/%'",
            [],
            |r| r.get::<_, usize>(0)
        )
        .unwrap(),
        0
    );
    let context:String=db.query_row("SELECT payload FROM records WHERE host=?1 AND login='user-id:42' AND kind='pr-metadata-context-v48' AND id='GENUINE_CANONICAL_PR_NODE_1007'",[HOST],|r|r.get(0)).unwrap();
    let context: Value = serde_json::from_str(&context).unwrap();
    let actual = after(command);
    for field in ["labels", "assignees", "milestone"] {
        assert_eq!(
            context["issue_metadata"][field], actual[field],
            "Exact refreshed structured field {field}, including all unmodified/opposite members"
        );
    }
    assert_eq!(
        context["issue_metadata"]["id"], 2007,
        "Issue database ID is independently verified, not equated with PR1007"
    );
    let requested_users = context["requested_reviewers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|actor| actor["login"].as_str().unwrap())
        .collect::<Vec<_>>();
    let expected_users = actual["requested_reviewers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|actor| actor["login"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(requested_users, expected_users);
    let requested_teams = context["requested_teams"]
        .as_array()
        .unwrap()
        .iter()
        .map(|team| (team["id"].as_u64().unwrap(), team["slug"].as_str().unwrap()))
        .collect::<Vec<_>>();
    let expected_teams = actual["requested_teams"]
        .as_array()
        .unwrap()
        .iter()
        .map(|team| (team["id"].as_u64().unwrap(), team["slug"].as_str().unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(requested_teams, expected_teams);
}
fn resize_rejects_hidden_confirm_v191(
    s: &mut Session,
    f: &Fixture,
    mouse: bool,
    prepared: &[GhStep],
) {
    fn sql(f: &Fixture) -> std::collections::BTreeMap<String, Vec<Vec<rusqlite::types::Value>>> {
        let db = super::raw(f);
        let names = db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap()
            .query_map([], |row| row.get::<_, String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>();
        names
            .into_iter()
            .map(|name| {
                let mut statement = db
                    .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
                    .unwrap();
                let columns = statement.column_count();
                let mut rows = statement
                    .query_map([], |row| {
                        (0..columns)
                            .map(|column| row.get(column))
                            .collect::<rusqlite::Result<Vec<_>>>()
                    })
                    .unwrap()
                    .map(Result::unwrap)
                    .collect::<Vec<_>>();
                rows.sort_by_key(|row| format!("{row:?}"));
                (name, rows)
            })
            .collect()
    }
    let before_sql = sql(f);
    let before_git = git_state(f);
    let before_commands = fs::read(f.root.join("gh-fixture/commands.jsonl")).unwrap();
    let resize = s.completed_cells_end();
    s.resize(40, 10);
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > resize
            && s.completed_cells_screen().is_some_and(|frame| {
                frame.size() == (10, 40)
                    && crate::support::overlay_body(frame, "Preview • resize required")
                        .is_some_and(|body| body.contains("[Ctrl+Enter Confirm unavailable]"))
            })
        {
            break;
        }
        assert!(
            Instant::now() < until,
            "NEW tiny terminal must visibly disable hidden Members confirmation\n{}",
            s.screen()
        );
    }
    let input = s.completed_cells_end();
    if mouse {
        let (x, y) = crate::support::completed_navigation_controls_v1::owned_action_position(
            s.completed_cells_screen().unwrap(),
            "Preview • resize required",
            "[Ctrl+Enter Confirm unavailable]",
            &[],
        )
        .expect("Click the actual visible unavailable control in this owned resize box");
        s.click(x, y);
    } else {
        s.send_key(b"\x1b[13;5u");
    }
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > input
            && s.completed_cells_screen().is_some_and(|frame| {
                frame.size() == (10, 40)
                    && crate::support::overlay_body(frame, "Preview • resize required").is_some_and(
                        |body| {
                            body.contains("Confirm refused")
                                && body.contains("[Ctrl+Enter Confirm unavailable]")
                        },
                    )
            })
        {
            break;
        }
        assert!(Instant::now() < until, "Hidden keyboard or visible unavailable mouse Confirm must be refused in a NEW frame\n{}", s.screen());
    }
    assert_eq!(
        fs::read(f.root.join("gh-fixture/commands.jsonl")).unwrap(),
        before_commands
    );
    assert_eq!(
        sql(f),
        before_sql,
        "No claim, draft, epoch, membership, receipt or unrelated SQL mutation on rejected Confirm"
    );
    assert_eq!(git_state(f), before_git);
    exact(s, f, prepared);
    s.capture_completed_terminal_v1("metadata-members-tiny-resize-confirm-refused-no-effects");
}
fn happy(mouse: bool, command: Cmd) {
    for personal in [false, true] {
        let mut steps = initial_steps(personal);
        let initial = steps.len();
        steps.extend(phase(command, raw(), candidate(command), current()));
        let opened = steps.len();
        steps.extend(selected_phase(command));
        let prepared = steps.len();
        steps.extend(selected_phase(command));
        steps.extend(post(command));
        let (f, mut s, git, config, stable) = begin(&steps, personal, mouse, initial);
        open_members(&mut s, &f, mouse, command, opened);
        select(&mut s, &f, mouse, command, &steps[..opened]);
        prepare(&mut s, &f, mouse, command, prepared);
        if command == Cmd::AddLabel {
            resize_rejects_hidden_confirm_v191(&mut s, &f, mouse, &steps[..prepared]);
        }
        resize_members(&mut s, command, 120, 40, PREVIEW_CONTROLS);
        resize_members(&mut s, command, 80, 24, PREVIEW_CONTROLS);
        control(
            &mut s,
            mouse,
            &members_title(command),
            "[Confirm action]",
            PREVIEW_CONTROLS,
        );
        s.wait_for_command_count(&f, steps.len());
        refreshed_v19::wait_fresh_page_v19(&mut s);
        receipt(&f, command);
        let after = s.completed_cursorless_end();
        choose(&mut s, mouse, "[b Back]", b"b");
        s.wait_for_new_cursorless_condition(
            after,
            |screen| {
                crate::support::main_footer_visible(screen)
                    && !screen.contents().contains("[b Back]")
            },
            "Pure same-family refreshed Back; zero HTTP",
            Duration::from_secs(8),
        );
        finish(s, &f, mouse, &steps, &git, &config, &stable);
    }
}
macro_rules! member_native {
    ($key:ident,$mouse:ident,$command:ident) => {
        #[test]
        fn $key() {
            happy(false, Cmd::$command)
        }
        #[test]
        fn $mouse() {
            happy(true, Cmd::$command)
        }
    };
}
member_native!(
    pr_metadata_members_v50_add_label_keyboard,
    pr_metadata_members_v50_add_label_mouse,
    AddLabel
);
member_native!(
    pr_metadata_members_v50_remove_label_keyboard,
    pr_metadata_members_v50_remove_label_mouse,
    RemoveLabel
);
member_native!(
    pr_metadata_members_v50_add_assignee_keyboard,
    pr_metadata_members_v50_add_assignee_mouse,
    AddAssignee
);
member_native!(
    pr_metadata_members_v50_remove_assignee_keyboard,
    pr_metadata_members_v50_remove_assignee_mouse,
    RemoveAssignee
);
member_native!(
    pr_metadata_members_v50_add_user_reviewer_keyboard,
    pr_metadata_members_v50_add_user_reviewer_mouse,
    AddReviewer
);
member_native!(
    pr_metadata_members_v50_remove_user_reviewer_keyboard,
    pr_metadata_members_v50_remove_user_reviewer_mouse,
    RemoveReviewer
);
member_native!(
    pr_metadata_members_v50_add_team_keyboard,
    pr_metadata_members_v50_add_team_mouse,
    AddTeam
);
member_native!(
    pr_metadata_members_v50_remove_team_keyboard,
    pr_metadata_members_v50_remove_team_mouse,
    RemoveTeam
);
member_native!(
    pr_metadata_members_v50_set_milestone_keyboard,
    pr_metadata_members_v50_set_milestone_mouse,
    SetMilestone
);
member_native!(
    pr_metadata_members_v50_clear_milestone_keyboard,
    pr_metadata_members_v50_clear_milestone_mouse,
    ClearMilestone
);

fn no_attempt(f: &Fixture) {
    assert_eq!(
        super::raw(f)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                .get::<_, usize>(0))
            .unwrap(),
        0
    );
}
fn close_members(s: &mut Session, mouse: bool, command: Cmd, preview: bool) {
    let after = s.completed_cursorless_end();
    control(
        s,
        mouse,
        &members_title(command),
        if preview {
            "[Keep draft / Close]"
        } else {
            "[Esc Close]"
        },
        if preview {
            PREVIEW_CONTROLS
        } else {
            selection_controls(command)
        },
    );
    s.wait_for_new_cursorless_condition(
        after,
        |screen| {
            crate::support::main_footer_visible(screen)
                && !screen.contents().contains(&members_title(command))
                && !screen.contents().contains("[Confirm action]")
        },
        "Exact local Members Close, no invocation or stale private Form",
        Duration::from_secs(8),
    );
}
fn invalid_selection(mouse: bool) {
    for personal in [false, true] {
        let command = Cmd::AddAssignee;
        let mut steps = initial_steps(personal);
        let initial = steps.len();
        steps.extend(phase(command, raw(), candidate(command), current()));
        let opened = steps.len();
        steps.extend(selected_phase(command));
        let prepared = steps.len();
        let (f, mut s, git, config, stable) = begin(&steps, personal, mouse, initial);
        open_members(&mut s, &f, mouse, command, opened);
        let sql = complete_sql_v46(&f);
        let after = s.completed_cells_end();
        control(
            &mut s,
            mouse,
            &members_title(command),
            "[Preview]",
            SELECT_CONTROLS,
        );
        form_frame(
            &mut s,
            after,
            &members_title(command),
            &[
                "Choose an actual indexed member before Preview",
                "[Choose member]",
                "[Preview]",
            ],
        );
        no_attempt(&f);
        assert_eq!(complete_sql_v46(&f), sql);
        exact(&s, &f, &steps[..opened]);
        select(&mut s, &f, mouse, command, &steps[..opened]);
        prepare(&mut s, &f, mouse, command, prepared);
        let sql = complete_sql_v46(&f);
        resize_members(&mut s, command, 80, 24, PREVIEW_CONTROLS);
        resize_members(&mut s, command, 120, 40, PREVIEW_CONTROLS);
        close_members(&mut s, mouse, command, true);
        assert_eq!(complete_sql_v46(&f), sql);
        no_attempt(&f);
        finish(s, &f, mouse, &steps, &git, &config, &stable);
    }
}
fn archive_unknown(mouse: bool) {
    for personal in [false, true] {
        let command = Cmd::AddLabel;
        let mut option = candidate(command);
        option.as_object_mut().unwrap().remove("archived_at");
        let mut steps = initial_steps(personal);
        let initial = steps.len();
        steps.extend(phase(command, raw(), option, current()));
        let end = steps.len();
        let (f, mut s, git, config, stable) = begin(&steps, personal, mouse, initial);
        open_members(&mut s, &f, mouse, command, end);
        let sql = complete_sql_v46(&f);
        let after = s.completed_cells_end();
        control(
            &mut s,
            mouse,
            &members_title(command),
            "[Choose member]",
            SELECT_CONTROLS,
        );
        form_frame(
            &mut s,
            after,
            picker_title(command),
            &["new-label", "[Enter Choose]", "[Esc Back]"],
        );
        let shown = crate::support::overlay_body(
            s.completed_cells_screen().unwrap(),
            picker_title(command),
        )
        .unwrap();
        assert!(
            shown.contains("archive evidence unavailable") || shown.contains("Add is disabled"),
            "Actual missing archived_at is not explicit JSON null evidence"
        );
        let after = s.completed_cells_end();
        if mouse {
            let point =
                s.wait_for_owned_completed_action(picker_title(command), "[Enter Choose]", &[]);
            s.click(point.0, point.1);
        } else {
            s.send_key(b"\r");
        }
        form_frame(
            &mut s,
            after,
            picker_title(command),
            &["new-label", "[Esc Back]"],
        );
        assert_eq!(complete_sql_v46(&f), sql);
        no_attempt(&f);
        exact(&s, &f, &steps);
        let after = s.completed_cells_end();
        picker_control(&mut s, mouse, command, "[Esc Back]");
        form_frame(&mut s, after, &members_title(command), SELECT_CONTROLS);
        close_members(&mut s, mouse, command, false);
        finish(s, &f, mouse, &steps, &git, &config, &stable);
    }
}
fn denied(mouse: bool) {
    for personal in [false, true] {
        let command = Cmd::AddReviewer;
        let mut calls = phase(command, raw(), candidate(command), current());
        for index in [2, 8] {
            let mut repository = repo();
            repository["permissions"] =
                json!({"pull":true,"push":false,"maintain":false,"admin":false});
            calls[index].stdout = serde_json::to_vec(&repository).unwrap();
        }
        let mut steps = initial_steps(personal);
        let initial = steps.len();
        steps.extend(calls);
        let end = steps.len();
        let (f, mut s, git, config, stable) = begin(&steps, personal, mouse, initial);
        open_members(&mut s, &f, mouse, command, end);
        let sql = complete_sql_v46(&f);
        let after = s.completed_cells_end();
        control(
            &mut s,
            mouse,
            &members_title(command),
            "[Choose member]",
            SELECT_CONTROLS,
        );
        form_frame(
            &mut s,
            after,
            picker_title(command),
            &["new-reviewer", "[Esc Back]"],
        );
        let shown = crate::support::overlay_body(
            s.completed_cells_screen().unwrap(),
            picker_title(command),
        )
        .unwrap();
        assert!(
            shown.contains("mutation permission") || shown.contains("browse only"),
            "Candidate READ access is not requester write permission"
        );
        let after = s.completed_cells_end();
        picker_control(&mut s, mouse, command, "[Esc Back]");
        form_frame(&mut s, after, &members_title(command), SELECT_CONTROLS);
        no_attempt(&f);
        assert_eq!(complete_sql_v46(&f), sql);
        close_members(&mut s, mouse, command, false);
        finish(s, &f, mouse, &steps, &git, &config, &stable);
    }
}
fn account_loss(mouse: bool) {
    for personal in [false, true] {
        let command = Cmd::RemoveLabel;
        let mut steps = initial_steps(personal);
        let initial = steps.len();
        steps.extend(phase(command, raw(), candidate(command), current()));
        let opened = steps.len();
        steps.extend(principal(43));
        let (f, mut s, git, config, stable) = begin(&steps, personal, mouse, initial);
        open_members(&mut s, &f, mouse, command, opened);
        select(&mut s, &f, mouse, command, &steps[..opened]);
        let sql = complete_sql_v46(&f);
        let after = s.completed_cursorless_end();
        control(
            &mut s,
            mouse,
            &members_title(command),
            "[Preview]",
            SELECT_CONTROLS,
        );
        s.wait_for_command_count(&f, steps.len());
        s.wait_for_new_cursorless_condition(after,|screen|crate::support::main_footer_visible(screen)&&!screen.contents().contains(&members_title(command))&&!screen.contents().contains(DETAIL_BODY)&&!screen.contents().contains("[Choose member]"),"Genuine fresh numeric-account loss hides exact private Members Form and original underlay",Duration::from_secs(8));
        no_attempt(&f);
        assert_eq!(complete_sql_v46(&f), sql);
        finish(s, &f, mouse, &steps, &git, &config, &stable);
    }
}
fn held_cancel(mouse: bool) {
    for personal in [false, true] {
        let command = Cmd::AddLabel;
        let mut steps = initial_steps(personal);
        let initial = steps.len();
        steps.extend(phase(command, raw(), candidate(command), current()));
        let opened = steps.len();
        steps.extend(selected_phase(command));
        let prepared = steps.len();
        steps.extend(selected_phase(command));
        let paid = post(command);
        steps.push(paid[0].clone());
        let held_index = steps.len();
        let mut held = paid[1].clone();
        held.delay_ms = 30_000;
        steps.push(held);
        let (f, mut s, git, config, stable) = begin(&steps, personal, mouse, initial);
        open_members(&mut s, &f, mouse, command, opened);
        select(&mut s, &f, mouse, command, &steps[..opened]);
        prepare(&mut s, &f, mouse, command, prepared);
        control(
            &mut s,
            mouse,
            &members_title(command),
            "[Confirm action]",
            PREVIEW_CONTROLS,
        );
        s.wait_for_command_count(&f, steps.len());
        let rows = commands(&s, &f);
        let pid = i32::try_from(rows[held_index]["pid"].as_u64().unwrap()).unwrap();
        let identity = process_identity(pid).unwrap().unwrap();
        let owned = OwnedPtySession::capture(s.owned_application_pid() as u32).unwrap();
        assert!(owned.owns(&identity));
        assert!(!identity.zombie);
        let db = super::raw(&f);
        let envelope: String = db
            .query_row(
                "SELECT body FROM drafts WHERE resource LIKE 'metadata-members-v48/%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let origin:String=db.query_row("SELECT json_array(attempt_id,repository_id,pr_id,origin_json,base_key_v48,action_tag_v48,parent_ack_attempt_v48) FROM pr_write_metadata_origins_v25",[],|r|r.get(0)).unwrap();
        let saved: Value = serde_json::from_str(&envelope).unwrap();
        assert!(saved["publication"].is_object());
        assert!(saved.get("approval").is_none());
        let before:String=db.query_row("SELECT json_array(attempt_id,host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at) FROM pr_write_attempts",[],|r|r.get(0)).unwrap();
        let mut expected: Value = serde_json::from_str(&before).unwrap();
        assert_eq!(expected[10], "pending");
        expected[10] = json!("uncertain");
        control(
            &mut s,
            mouse,
            &members_title(command),
            "[Esc Cancel]",
            &["[Esc Cancel]"],
        );
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            s.poll_output(Duration::from_millis(2));
            let gone = match process_identity(pid) {
                Ok(None) => true,
                Ok(Some(now)) => {
                    (now.uid, now.birth, now.session)
                        != (identity.uid, identity.birth, identity.session)
                }
                Err(e) if e.kind() == std::io::ErrorKind::InvalidData => false,
                Err(e) => panic!("Exact held Members child observation failed: {e}"),
            };
            let row:String=db.query_row("SELECT json_array(attempt_id,host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at) FROM pr_write_attempts",[],|r|r.get(0)).unwrap();
            let row: Value = serde_json::from_str(&row).unwrap();
            assert_eq!(
                db.query_row(
                    "SELECT body FROM drafts WHERE resource LIKE 'metadata-members-v48/%'",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
                envelope
            );
            if gone && row[10] == "uncertain" {
                assert_eq!(row, expected);
                break;
            }
            assert!(row[10] == "pending" || row == expected);
            assert!(Instant::now()<until,"Owned child must reap and actual same worker persist uncertainty within original5s");
        }
        assert_eq!(db.query_row("SELECT json_array(attempt_id,repository_id,pr_id,origin_json,base_key_v48,action_tag_v48,parent_ack_attempt_v48) FROM pr_write_metadata_origins_v25",[],|r|r.get::<_,String>(0)).unwrap(),origin);
        s.wait_for_cursorless_condition(
            |screen| {
                crate::support::main_footer_visible(screen)
                    && !screen.contents().contains(&members_title(command))
                    && !screen.contents().contains(DETAIL_BODY)
            },
            "Canceled Members private presentation retired; protected original retained",
            Duration::from_secs(8),
        );
        finish(s, &f, mouse, &steps, &git, &config, &stable);
    }
}
macro_rules! failure_native {
    ($key:ident,$mouse:ident,$helper:ident) => {
        #[test]
        fn $key() {
            $helper(false)
        }
        #[test]
        fn $mouse() {
            $helper(true)
        }
    };
}
failure_native!(
    pr_metadata_members_v50_selection_required_corrected_preview_keyboard,
    pr_metadata_members_v50_selection_required_corrected_preview_mouse,
    invalid_selection
);
failure_native!(
    pr_metadata_members_v50_archive_absent_picker_refuses_keyboard,
    pr_metadata_members_v50_archive_absent_picker_refuses_mouse,
    archive_unknown
);
failure_native!(
    pr_metadata_members_v50_candidate_read_is_not_requester_permission_keyboard,
    pr_metadata_members_v50_candidate_read_is_not_requester_permission_mouse,
    denied
);
failure_native!(
    pr_metadata_members_v50_current_numeric_account_loss_privacy_keyboard,
    pr_metadata_members_v50_current_numeric_account_loss_privacy_mouse,
    account_loss
);
failure_native!(
    pr_metadata_members_v50_held_readback_cancel_exact_owned_cleanup_keyboard,
    pr_metadata_members_v50_held_readback_cancel_exact_owned_cleanup_mouse,
    held_cancel
);
