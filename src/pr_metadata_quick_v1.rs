//! One-step label/assignee/reviewer/milestone controls on the PR Metadata tab.
//!
//! Each footer key reuses the exact action the PR actions menu (`p`) exposes for
//! the same command, so the key, the click and the menu share one workflow:
//! choose a value, Preview, then explicitly Confirm.
use crate::{
    app::{Action, App},
    footer::KeyControl,
    github_workspace::PrTab,
    pr_metadata_actions::PrMetadataCommand,
};

/// (command, shortcut key, visible label, compact label for narrow terminals)
pub const CONTROLS: [(PrMetadataCommand, char, &str, &str); 10] = [
    (PrMetadataCommand::AddLabel, 'l', "Add label", "+Label"),
    (
        PrMetadataCommand::RemoveLabel,
        'L',
        "Remove label",
        "-Label",
    ),
    (PrMetadataCommand::AddAssignee, 'a', "Assign", "+Assignee"),
    (
        PrMetadataCommand::RemoveAssignee,
        'A',
        "Unassign",
        "-Assignee",
    ),
    (
        PrMetadataCommand::AddReviewer,
        'w',
        "Request review",
        "+Reviewer",
    ),
    (
        PrMetadataCommand::RemoveReviewer,
        'W',
        "Remove reviewer",
        "-Reviewer",
    ),
    (PrMetadataCommand::AddTeam, 'g', "Request team", "+Team"),
    (PrMetadataCommand::RemoveTeam, 'G', "Remove team", "-Team"),
    (
        PrMetadataCommand::SetMilestone,
        'm',
        "Set milestone",
        "Milestone",
    ),
    (
        PrMetadataCommand::ClearMilestone,
        'M',
        "Clear milestone",
        "-Milestone",
    ),
];
/// Footer priority per command: the commonly used additions stay on narrow rows, removals
/// leave first. Every command is also an entry of the `p` menu.
fn priority(command: PrMetadataCommand) -> u8 {
    match command {
        PrMetadataCommand::AddLabel => 20,
        PrMetadataCommand::AddReviewer => 21,
        PrMetadataCommand::AddAssignee => 22,
        PrMetadataCommand::SetMilestone => 23,
        PrMetadataCommand::RemoveLabel => 30,
        PrMetadataCommand::RemoveReviewer => 31,
        PrMetadataCommand::RemoveAssignee => 32,
        PrMetadataCommand::ClearMilestone => 33,
        PrMetadataCommand::AddTeam => 34,
        PrMetadataCommand::RemoveTeam => 35,
    }
}

/// Visible only on an observed Metadata tab, where the `p` menu is also offered.
pub fn available(app: &App) -> bool {
    app.overlay.is_none()
        && app.github.as_ref().is_some_and(|github| {
            github.pr.as_ref().is_some_and(|page| {
                page.tab == PrTab::Metadata && page.metadata_display_v51.is_some()
            })
        })
}

/// The same action the PR actions menu holds for this command (including its
/// disabled explanation when the command is not currently allowed).
pub fn action(app: &App, command: PrMetadataCommand) -> Action {
    find(&app.pr_controls(), command)
}
fn find(controls: &[(String, Action)], command: PrMetadataCommand) -> Action {
    controls
        .iter()
        .find(|(label, _)| label == command.label())
        .map_or_else(
            || Action::Explain(format!("{} is not available for this PR.", command.label())),
            |(_, action)| action.clone(),
        )
}

/// Footer keys for the Metadata tab, each holding the same action as the matching `p` menu
/// entry. A refused command leaves the bar and its key explains why. Empty unless the
/// Metadata tab is showing.
pub fn footer_keys(app: &App) -> Vec<KeyControl> {
    if !available(app) {
        return Vec::new();
    }
    let menu = app.pr_controls();
    CONTROLS
        .iter()
        .map(|(command, key, text, compact)| {
            let action = find(&menu, *command);
            let reason = match &action {
                Action::Explain(reason) => Some(reason.clone()),
                _ => None,
            };
            KeyControl::new(key.to_string(), *text, action)
                .with_compact(*compact)
                .with_priority(priority(*command))
                .disabled_by(reason)
        })
        .collect()
}

#[cfg(test)]
#[path = "pr_metadata_quick_v1_tests.rs"]
mod tests;
