//! Prepared file-provenance controls; no Git reads are performed while drawing.
use crate::{
    app::Action,
    workspace::{FileAction, ProvenanceView, RowAction, Workspace},
};

pub(super) struct FileControl {
    pub label: &'static str,
    pub action: Action,
    pub selected: bool,
    pub disabled: bool,
}

pub(super) fn controls(workspace: &Workspace, busy: bool) -> Vec<FileControl> {
    let controls = if workspace
        .file_provenance
        .as_ref()
        .is_some_and(|state| state.view == ProvenanceView::Detail)
    {
        vec![
            ("[u File back]", FileAction::Back),
            ("[G Responsible graph]", FileAction::Graph),
            ("[D Introduced diff]", FileAction::Diff),
            ("[F Original file]", FileAction::OriginalFile),
        ]
    } else {
        vec![
            ("[H History]", FileAction::History),
            ("[Y Blame]", FileAction::Blame),
            ("[u File back]", FileAction::Back),
            ("[P Previous]", FileAction::Previous),
            ("[N Next]", FileAction::Next),
        ]
    };
    controls
        .into_iter()
        .map(|(label, file_action)| {
            let reason = if busy {
                Some("Wait for the current repository read, or cancel it first".into())
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
            FileControl {
                label,
                disabled: reason.is_some(),
                action: reason.map_or(
                    Action::RepositoryRow(RowAction::FileProvenance(file_action)),
                    Action::Explain,
                ),
                selected,
            }
        })
        .collect()
}
