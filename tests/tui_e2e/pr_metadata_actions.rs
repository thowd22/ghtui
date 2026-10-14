//! Registered native ten-operation journeys. TASK-18 records actual code/provider
//! and real-terminal results; strict protocol85 and independent effects remain checked.
use super::*;
use ghtui::{
    self as model,
    pr_metadata_actions::{PrMetadataCommand, PrMetadataReceipt},
};
#[path = "pr_metadata_actions_boundaries.rs"]
mod boundaries;
#[path = "pr_metadata_actions_crash.rs"]
mod crash;
#[path = "pr_metadata_actions_extended.rs"]
mod extended;
#[path = "pr_metadata_actions_label_query.rs"]
mod label_query;
#[path = "pr_metadata_actions_fixtures.rs"]
mod values;

fn title(command: PrMetadataCommand) -> &'static str {
    match command {
        PrMetadataCommand::AddLabel => "Add label",
        PrMetadataCommand::RemoveLabel => "Remove label",
        PrMetadataCommand::AddAssignee => "Add assignee",
        PrMetadataCommand::RemoveAssignee => "Remove assignee",
        PrMetadataCommand::AddReviewer => "Request user review",
        PrMetadataCommand::RemoveReviewer => "Remove user review request",
        PrMetadataCommand::AddTeam => "Request team review",
        PrMetadataCommand::RemoveTeam => "Remove team review request",
        PrMetadataCommand::SetMilestone => "Set milestone",
        PrMetadataCommand::ClearMilestone => "Clear milestone",
    }
}
fn selected_name(command: PrMetadataCommand) -> &'static str {
    match command {
        PrMetadataCommand::AddLabel => "new 🐛",
        PrMetadataCommand::RemoveLabel => "existing ☃",
        PrMetadataCommand::AddAssignee => "new-assignee",
        PrMetadataCommand::RemoveAssignee => "current-assignee",
        PrMetadataCommand::AddReviewer => "new-reviewer",
        PrMetadataCommand::RemoveReviewer => "current-reviewer",
        PrMetadataCommand::AddTeam => "new-team",
        PrMetadataCommand::RemoveTeam => "current-team",
        PrMetadataCommand::SetMilestone => "Milestone 6",
        PrMetadataCommand::ClearMilestone => "Milestone 5",
    }
}
fn bound(mut steps: Vec<GhStep>, current: &Value) -> Vec<GhStep> {
    for step in &mut steps {
        for arg in &mut step.args {
            *arg = arg
                .replace("author:octocat", "author:requester")
                .replace("assignee:octocat", "assignee:requester")
                .replace("review-requested:octocat", "review-requested:requester");
        }
        if step.args.get(1).is_some_and(|arg| arg == "user") {
            step.stdout = serde_json::to_vec(&json!({"id":42,"login":"requester"})).unwrap();
            continue;
        }
        if let Ok(mut value) = serde_json::from_slice::<Value>(&step.stdout) {
            substitute(&mut value, current);
            if step
                .args
                .get(1)
                .is_some_and(|arg| arg == "repos/example/repo/issues/7")
            {
                value = values::issue_from(current);
            } else if step
                .args
                .get(1)
                .is_some_and(|arg| arg == "repos/example/repo")
            {
                value = values::repository();
            } else if step.args.get(1).is_some_and(|arg| arg == "search/issues")
                && step.args.iter().any(|arg| {
                    arg.contains("assignee:requester") || arg.contains("review-requested:requester")
                })
            {
                // The fixture PR is genuinely authored by42, not assigned to or
                // review-requested from42. Do not invent personal queue facts.
                value["total_count"] = json!(0);
                value["items"] = json!([]);
            }
            step.stdout = serde_json::to_vec(&value).unwrap();
        }
    }
    steps
}
fn current_parent() -> Value {
    let mut value = values::parent();
    value["repository_url"] = json!("https://github.invalid/api/v3/repos/example/repo");
    value["locked"] = json!(false);
    value
}
fn after(command: PrMetadataCommand) -> Value {
    let mut value = values::changed(command);
    value["repository_url"] = json!("https://github.invalid/api/v3/repos/example/repo");
    value["locked"] = json!(false);
    value
}
fn startup(current: &Value) -> Vec<GhStep> {
    let mut steps = bound(initial(), current);
    steps.extend(bound(details(vec![starter()]), current));
    steps.extend(bound(metadata(current), current));
    assert_eq!(steps.len(), 18);
    steps
}
fn source(command: PrMetadataCommand, current: &Value) -> Vec<GhStep> {
    let mut steps = values::catalogue(command, current, vec![values::candidate(command)]);
    steps.extend(bound(identity(), current));
    assert_eq!(steps.len(), 10);
    steps
}
fn publication(command: PrMetadataCommand, before: &Value, after: &Value) -> Vec<GhStep> {
    let mut steps = bound(identity(), before);
    let preparation = values::preparation_from(command, before);
    assert_eq!(preparation.len(), 12);
    steps.extend(preparation);
    let dispatch = values::publication_from(command, after, values::mutation_input(command));
    assert_eq!(dispatch.len(), 6);
    steps.extend(dispatch);
    steps.extend(bound(inspected(vec![starter()]), after));
    steps.extend(bound(metadata(after), after));
    steps.extend(bound(identity(), after));
    steps.extend(bound(identity(), after));
    assert_eq!(steps.len(), 39);
    steps
}
fn open_pr(session: &mut Session, mouse: bool) {
    session.wait_for_screen("ghtui");
    session.send_key(b"1");
    choose(session, mouse, "[r Refresh]", b"r");
    session.wait_for_screen("Unchanged title");
    session.wait_for_screen("Fresh • fetched");
    choose(session, mouse, "Unchanged title", b"\r");
    session.wait_for_screen("1 comments");
    choose(session, mouse, "[u Metadata]", b"u");
    session.wait_for_screen("Your repository permissions:");
}
fn ready_form(session: &mut Session, command: PrMetadataCommand) {
    session
        .try_wait_for_condition(
            |screen| {
                screen.contains(title(command))
                    && screen.contains("Selected value:")
                    && screen.contains("[F4 Choose]")
                    && screen.contains("[Keep draft / Close]")
                    && !screen.contains("[Esc Cancel]")
            },
            "complete original-bound metadata form",
            Duration::from_secs(8),
        )
        .unwrap();
}
fn open_form(session: &mut Session, mouse: bool, command: PrMetadataCommand) {
    menu(session, mouse, title(command));
    ready_form(session, command);
}
fn choose_value(
    session: &mut Session,
    fixture: &Fixture,
    mouse: bool,
    command: PrMetadataCommand,
    count: usize,
) {
    ready_form(session, command);
    choose(session, mouse, "[F4 Choose]", b"\x1bOS");
    let journal = records(session, fixture, count);
    assert_eq!(mutation_count(&journal), 0);
    if command != PrMetadataCommand::ClearMilestone {
        let heading = match command {
            PrMetadataCommand::RemoveLabel => "Current PR metadata members · Label",
            PrMetadataCommand::RemoveAssignee => "Current PR metadata members · Assignee",
            PrMetadataCommand::RemoveReviewer => "Current PR metadata members · Reviewer",
            PrMetadataCommand::RemoveTeam => "Current PR metadata members · Reviewer team",
            _ => "PR metadata choices",
        };
        session.wait_for_screen(heading);
        session.wait_for_screen(selected_name(command));
        if mouse {
            session.click_visible_action(selected_name(command));
            session.click_visible_action("[Enter Choose]");
        } else {
            session.focus_visible_form_action("[Enter Choose]");
            session.send_key(b"\r");
        }
        session.wait_for_overlay_close(heading);
    } else {
        session.wait_for_screen("Clear current milestone");
    }
    ready_form(session, command);
    session.wait_for_screen("PR INTENT • Preview first");
    assert_no_publication(fixture);
}
fn assert_no_publication(fixture: &Fixture) {
    // This independent oracle must not migrate/repair the live application's
    // database or race its transient journal cleanup merely to inspect rows.
    let sql = Connection::open_with_flags(
        cache_path(fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let exists:bool = sql.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='pr_write_attempts')",[],|r|r.get(0)).unwrap();
    if exists {
        let count: usize = sql
            .query_row("SELECT COUNT(*) FROM pr_write_attempts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            count, 0,
            "Read/selection/cancellation/blocked Confirm cannot create any intent"
        );
    }
}
fn close_form(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[Keep draft / Close]", b"\x1b");
    session
        .try_wait_for_condition(
            |screen| {
                screen.contains("[u Metadata]")
                    && !screen.contains("[Keep draft / Close]")
                    && !screen.contains("[Esc Cancel]")
                    && !screen.contains("[F4 Choose]")
            },
            "complete Metadata after form close",
            Duration::from_secs(8),
        )
        .unwrap();
}
fn observed_metadata(session: &mut Session, mouse: bool, current: &Value) {
    observe(session, mouse, "Your repository permissions:");
    let milestone = current["milestone"]["title"].as_str().unwrap_or("none");
    observe(session, mouse, &format!("Milestone: {milestone}"));
    for label in current["labels"].as_array().unwrap() {
        observe(
            session,
            mouse,
            &format!("Label: {}", label["name"].as_str().unwrap()),
        );
    }
    for assignee in current["assignees"].as_array().unwrap() {
        observe(session, mouse, assignee["login"].as_str().unwrap());
    }
    for reviewer in current["requested_reviewers"].as_array().unwrap() {
        observe(session, mouse, reviewer["login"].as_str().unwrap());
    }
    for team in current["requested_teams"].as_array().unwrap() {
        observe(
            session,
            mouse,
            &format!(
                "Review team: {} ({})",
                team["name"].as_str().unwrap(),
                team["slug"].as_str().unwrap()
            ),
        );
    }
}
fn assert_receipt(fixture: &Fixture, command: PrMetadataCommand, after: &Value) {
    let sql = Connection::open(cache_path(fixture)).unwrap();
    let row:(i64,String,String,String,String,String,String,String,u64,String)=sql.query_row(
        "SELECT attempt_id,host,principal,pr_node,command,state,selected_resource,repository,pr_number,confirmed_json FROM pr_write_attempts",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?,r.get(9)?))).unwrap();
    let count: usize = sql
        .query_row("SELECT COUNT(*) FROM pr_write_attempts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
    assert!(row.0 > 0);
    assert_eq!(
        (row.1.as_str(), row.2.as_str(), row.3.as_str()),
        ("github.invalid", "user-id:42", "PR_7")
    );
    assert_eq!(
        (row.4.as_str(), row.5.as_str(), row.7.as_str(), row.8),
        (command.key(), "confirmed", "example/repo", 7)
    );
    let effect: model::pr_write_journal::PrConfirmedEffect = serde_json::from_str(&row.9).unwrap();
    let decoded = PrMetadataReceipt::decode(&effect).unwrap();
    assert_eq!(decoded.command(), command);
    assert_eq!(decoded.repository_id(), 410);
    assert_eq!(effect.node_id.as_deref(), Some("PR_7"));
    assert!(effect
        .resource_id
        .starts_with(&format!("metadata-v2/{}/410/1007/2007/", command.key())));
    assert!(!row.9.contains("Private description") && !row.9.contains("requester"));
    let field = values::field(command);
    let rawkey = match field {
        "reviewers" => "requested_reviewers",
        "teams" => "requested_teams",
        other => other,
    };
    let expected = if field == "milestone" {
        after[rawkey]
            .as_object()
            .map(|_| {
                vec![(
                    after[rawkey]["id"].as_u64().unwrap(),
                    Some(after[rawkey]["number"].as_u64().unwrap()),
                )]
            })
            .unwrap_or_default()
    } else {
        after[rawkey]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| (r["id"].as_u64().unwrap(), None))
            .collect()
    };
    assert_eq!(decoded.result_ids(), expected);
    let selected = match command {
        PrMetadataCommand::AddLabel => 81,
        PrMetadataCommand::RemoveLabel => 80,
        PrMetadataCommand::AddAssignee => 61,
        PrMetadataCommand::RemoveAssignee => 60,
        PrMetadataCommand::AddReviewer => 71,
        PrMetadataCommand::RemoveReviewer => 70,
        PrMetadataCommand::AddTeam => 76,
        PrMetadataCommand::RemoveTeam => 75,
        PrMetadataCommand::SetMilestone => 86,
        PrMetadataCommand::ClearMilestone => 85,
    };
    assert_eq!(decoded.selected_id(), Some(selected));
    assert_eq!(row.6, format!("{field}:{selected}"));
    let opposite = if field == "reviewers" {
        vec![75]
    } else if field == "teams" {
        vec![70]
    } else {
        vec![]
    };
    assert_eq!(decoded.preserved_review_ids(), opposite);
    let oldhead: String = sql
        .query_row(
            "SELECT observed_head FROM pr_write_attempts WHERE attempt_id=?1",
            [row.0],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(oldhead, "a".repeat(40));
}
fn happy(command: PrMetadataCommand, mouse: bool) {
    let fixture = fixture();
    let original = current_parent();
    let changed = after(command);
    for key in [
        "id", "node_id", "number", "title", "body", "user", "head", "base", "draft", "state",
        "merged",
    ] {
        assert_eq!(
            original[key], changed[key],
            "Unmodified canonical parent field {key}"
        );
    }
    for key in [
        "labels",
        "assignees",
        "requested_reviewers",
        "requested_teams",
        "milestone",
    ] {
        let changedkey = match values::field(command) {
            "reviewers" => "requested_reviewers",
            "teams" => "requested_teams",
            other => other,
        };
        if key != changedkey {
            assert_eq!(original[key], changed[key], "Unmodified metadata {key}");
        }
    }
    let mut steps = startup(&original);
    steps.extend(bound(identity(), &original));
    steps.extend(bound(identity(), &original));
    steps.extend(source(command, &original));
    steps.extend(bound(identity(), &original));
    steps.extend(bound(identity(), &original));
    steps.extend(source(command, &original));
    steps.extend(publication(command, &original, &changed));
    assert_eq!(steps.len(), 85);
    fixture.scripted_gh(&steps);
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let mut session = Session::launch(&fixture, 120, 40);
    open_pr(&mut session, mouse);
    observed_metadata(&mut session, mouse, &original);
    assert_eq!(records(&mut session, &fixture, 18).len(), 18);
    open_form(&mut session, mouse, command);
    choose_value(&mut session, &fixture, mouse, command, 32);
    confirm(&mut session, mouse);
    session.wait_for_screen("Choose Preview");
    assert_no_publication(&fixture);
    assert_eq!(mutation_count(&records(&mut session, &fixture, 32)), 0);
    close_form(&mut session, mouse);
    preserve_git(&fixture, &head, &index, &status);
    session.resize(80, 24);
    open_form(&mut session, mouse, command);
    choose_value(&mut session, &fixture, mouse, command, 46);
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    if command.aggregate_review_replacement() {
        observe(&mut session, mouse, "replacement");
    }
    assert_eq!(mutation_count(&records(&mut session, &fixture, 46)), 0);
    assert_no_publication(&fixture);
    confirm(&mut session, mouse);
    let observed = records(&mut session, &fixture, 85);
    assert_eq!(mutation_count(&observed), 1);
    let written = observed.iter().find(|r| is_mutation(r)).unwrap();
    let bytes = written["stdin_hex"]
        .as_str()
        .unwrap()
        .as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect::<Vec<_>>();
    let actual: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        actual["variables"]["input"],
        values::mutation_input(command)
    );
    assert_eq!(
        written["args"],
        json!([
            "api",
            "graphql",
            "--hostname",
            "github.invalid",
            "--method",
            "POST",
            "--input",
            "-"
        ])
    );
    session
        .try_wait_for_condition(
            |screen| {
                screen.contains("[View original]")
                    && screen.contains("[Keep draft / Close]")
                    && !screen.contains("[Esc Cancel]")
            },
            "completed metadata confirmation preserving original reference",
            Duration::from_secs(8),
        )
        .unwrap();
    assert_receipt(&fixture, command, &changed);
    preserve_git(&fixture, &head, &index, &status);
    close_form(&mut session, mouse);
    observed_metadata(&mut session, mouse, &changed);
    assert_eq!(mutation_count(&records(&mut session, &fixture, 85)), 1);
    preserve_git(&fixture, &head, &index, &status);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn pr_metadata_actions_add_label_keyboard() {
    happy(PrMetadataCommand::AddLabel, false);
}
#[test]
fn pr_metadata_actions_add_label_mouse() {
    happy(PrMetadataCommand::AddLabel, true);
}
#[test]
fn pr_metadata_actions_remove_label_keyboard() {
    happy(PrMetadataCommand::RemoveLabel, false);
}
#[test]
fn pr_metadata_actions_remove_label_mouse() {
    happy(PrMetadataCommand::RemoveLabel, true);
}
#[test]
fn pr_metadata_actions_add_assignee_keyboard() {
    happy(PrMetadataCommand::AddAssignee, false);
}
#[test]
fn pr_metadata_actions_add_assignee_mouse() {
    happy(PrMetadataCommand::AddAssignee, true);
}
#[test]
fn pr_metadata_actions_remove_assignee_keyboard() {
    happy(PrMetadataCommand::RemoveAssignee, false);
}
#[test]
fn pr_metadata_actions_remove_assignee_mouse() {
    happy(PrMetadataCommand::RemoveAssignee, true);
}
#[test]
fn pr_metadata_actions_add_reviewer_keyboard() {
    happy(PrMetadataCommand::AddReviewer, false);
}
#[test]
fn pr_metadata_actions_add_reviewer_mouse() {
    happy(PrMetadataCommand::AddReviewer, true);
}
#[test]
fn pr_metadata_actions_remove_reviewer_keyboard() {
    happy(PrMetadataCommand::RemoveReviewer, false);
}
#[test]
fn pr_metadata_actions_remove_reviewer_mouse() {
    happy(PrMetadataCommand::RemoveReviewer, true);
}
#[test]
fn pr_metadata_actions_add_team_keyboard() {
    happy(PrMetadataCommand::AddTeam, false);
}
#[test]
fn pr_metadata_actions_add_team_mouse() {
    happy(PrMetadataCommand::AddTeam, true);
}
#[test]
fn pr_metadata_actions_remove_team_keyboard() {
    happy(PrMetadataCommand::RemoveTeam, false);
}
#[test]
fn pr_metadata_actions_remove_team_mouse() {
    happy(PrMetadataCommand::RemoveTeam, true);
}
#[test]
fn pr_metadata_actions_set_milestone_keyboard() {
    happy(PrMetadataCommand::SetMilestone, false);
}
#[test]
fn pr_metadata_actions_set_milestone_mouse() {
    happy(PrMetadataCommand::SetMilestone, true);
}
#[test]
fn pr_metadata_actions_clear_milestone_keyboard() {
    happy(PrMetadataCommand::ClearMilestone, false);
}
#[test]
fn pr_metadata_actions_clear_milestone_mouse() {
    happy(PrMetadataCommand::ClearMilestone, true);
}
