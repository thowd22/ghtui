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
#[path = "pr_metadata_actions_legacy_v96.rs"]
mod legacy_v96;
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
fn happy(command: PrMetadataCommand, mouse: bool) {
    legacy_v96::happy(command, mouse);
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
