//! One-step label/assignee/reviewer/milestone controls on the PR Metadata tab.
//!
//! Each control reuses the exact action the PR actions menu (`p`) exposes for
//! the same command, so the key, the click and the menu share one workflow:
//! choose a value, Preview, then explicitly Confirm.
use crate::{
    app::{Action, App},
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
/// Terminals at least this wide show the full labels.
pub const WIDE: u16 = 100;

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

pub fn label(key: char, text: &str) -> String {
    format!("[{key} {text}]")
}

/// Keyboard shortcut. Returns true when the key was handled.
pub fn key(app: &mut App, character: char) -> bool {
    if !available(app) {
        return false;
    }
    let Some((command, _, _, _)) = CONTROLS.iter().find(|(_, key, _, _)| *key == character) else {
        return false;
    };
    let action = action(app, *command);
    app.dispatch(action);
    true
}

/// Controls appended to the PR header buttons, each holding the same action
/// as the matching `p` menu entry. Empty unless the Metadata tab is showing.
pub fn controls(app: &App, width: u16) -> Vec<(String, Action)> {
    if !available(app) {
        return Vec::new();
    }
    let menu = app.pr_controls();
    CONTROLS
        .iter()
        .map(|(command, key, text, compact)| {
            (
                label(*key, if width >= WIDE { text } else { compact }),
                find(&menu, *command),
            )
        })
        .collect()
}

#[cfg(test)]
#[path = "pr_metadata_quick_v1_tests.rs"]
mod tests;
