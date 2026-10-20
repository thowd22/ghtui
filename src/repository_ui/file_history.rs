//! Prepared file-provenance key-bar controls; no Git reads are performed while drawing.
use crate::{
    app::Action,
    footer::{ControlGroup, KeyControl},
    workspace::{FileAction, ProvenanceView, RowAction, Workspace},
};

pub(super) const BUSY: &str = "Wait for the current repository read, or cancel it first";

/// The file-history keys in display order. Unavailable ones carry their reason, so they leave
/// the key bar while their key still explains why.
pub(super) fn controls(workspace: &Workspace, busy: bool) -> Vec<KeyControl> {
    let controls: Vec<(&str, &str, &str, FileAction, u8)> = if workspace
        .file_provenance
        .as_ref()
        .is_some_and(|state| state.view == ProvenanceView::Detail)
    {
        vec![
            ("u", "File back", "Back", FileAction::Back, 5),
            ("G", "Responsible graph", "Graph", FileAction::Graph, 20),
            ("D", "Introduced diff", "Diff", FileAction::Diff, 21),
            ("F", "Original file", "File", FileAction::OriginalFile, 22),
        ]
    } else {
        vec![
            // History with a `path:` chip for this file; `H` is a hand-written alias.
            ("f", "File history", "History", FileAction::PathHistory, 20),
            ("Y", "Blame", "Blame", FileAction::Blame, 21),
            ("u", "File back", "Back", FileAction::Back, 5),
            ("P", "Previous", "Prev", FileAction::Previous, 22),
            ("N", "Next", "Next", FileAction::Next, 23),
        ]
    };
    let tree_return = workspace.tree_file_return_v1().is_some();
    controls
        .into_iter()
        .map(|(key, label, compact, file_action, priority)| {
            if tree_return && file_action == FileAction::Back {
                // A file opened from the saved tree goes back to that tree.
                return KeyControl::new(
                    key,
                    "Back to tree",
                    Action::RepositoryMode(crate::workspace::RepositoryMode::Tree),
                )
                .with_compact("Tree")
                .with_priority(priority)
                .in_group(ControlGroup::Navigate)
                .disabled_by(busy.then(|| BUSY.to_string()));
            }
            let reason = if busy {
                Some(BUSY.to_string())
            } else {
                workspace.provenance_available(&file_action).err()
            };
            let selected = workspace.file_provenance.as_ref().is_some_and(|state| {
                matches!(
                    (&file_action, state.view),
                    (FileAction::History, ProvenanceView::History)
                        | (FileAction::Blame, ProvenanceView::Blame)
                )
            });
            let group = if file_action == FileAction::Back {
                ControlGroup::Navigate
            } else {
                ControlGroup::Act
            };
            KeyControl::new(
                key,
                label,
                Action::RepositoryRow(RowAction::FileProvenance(file_action)),
            )
            .with_compact(compact)
            .with_priority(priority)
            .in_group(group)
            .disabled_by(reason)
            .with_selected(selected)
        })
        .collect()
}
