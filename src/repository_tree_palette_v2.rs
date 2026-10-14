//! Finite contextual entries for the EXISTING palette; no picker engine/IO.
use crate::{
    app::{Action, App, View},
    command_palette::PaletteEntry,
    repository_tree_jump_v1::OriginalTreeJumpActionV1,
    repository_tree_queue_notice_v2::OriginalTreeQueueFailureV2,
    repository_tree_viewport_v2::{TreeFolderIntentV2, TreeViewportActionV2},
    workspace::RowAction,
};
use std::sync::Arc;

/// Root appends these after its existing global6/catalogue controls, then calls
/// its ordinary unique-ID/raw-payload preflight. Do not replace the catalogue.
pub fn entries_v2(
    app: &App,
    failure: Option<&Arc<OriginalTreeQueueFailureV2>>,
) -> Result<Vec<PaletteEntry>, String> {
    if app.view != View::Repository || app.remote_repository.is_some() {
        return Ok(vec![]);
    }
    let mut entries = vec![];
    let jump = app.workspace.as_ref().map_or_else(
        || Action::Explain("Open an original local saved Tree first".into()),
        |workspace| {
            if app.loading[View::Repository.index()] {
                Action::Explain(
                    "Wait for the original read or cancel explicitly before Jump".into(),
                )
            } else {
                crate::git::OriginalTreeIntentV1::available(workspace)
                    .map_or_else(Action::Explain, |_| {
                        Action::OriginalTreeV1(OriginalTreeJumpActionV1::Open)
                    })
            }
        },
    );
    entries.push(entry(
        "repository.tree.jump",
        "Jump to original saved path".into(),
        jump,
    ));
    for (id, label, delta) in [
        ("next", "Next100 original saved paths", 1),
        ("previous", "Previous100 original saved paths", -1),
    ] {
        let action = app.original_tree_jump_v1.as_ref().map_or_else(
            || Action::Explain("Open the original saved-path picker before paging".into()),
            |jump| {
                if jump.pending().is_some() || app.loading[View::Repository.index()] {
                    return Action::Explain(
                        "Original paths pending; no old page may be opened".into(),
                    );
                }
                let Some(frame) = jump.frame() else {
                    return Action::Explain(
                        "No current original path page; unseen paths unknown".into(),
                    );
                };
                if (delta > 0 && frame.offset_v2().saturating_add(100) >= frame.known_matches())
                    || (delta < 0 && frame.offset_v2() == 0)
                {
                    Action::Explain(
                        "No further loaded original path page; unseen paths remain unknown".into(),
                    )
                } else {
                    Action::OriginalTreeV1(OriginalTreeJumpActionV1::Page(delta))
                }
            },
        );
        entries.push(entry(
            &format!("repository.tree.{id}"),
            label.into(),
            action,
        ));
    }
    if let (Some(workspace), Some(jump), Some(failure)) =
        (&app.workspace, &app.original_tree_jump_v1, failure)
    {
        if failure.is_current(workspace, jump) {
            entries.push(entry(
                "repository.tree.retry",
                "Retry exact original saved-path request".into(),
                Action::OriginalTreeViewportV2(TreeViewportActionV2::Retry(failure.clone())),
            ));
        }
    }
    if let Some(projection) = app.workspace.as_ref().and_then(|workspace| {
        workspace
            .tree_navigation_v1
            .as_ref()
            .filter(|projection| projection.visible_for(workspace))
    }) {
        if projection.breadcrumbs().len() > 65 {
            return Err("Original folder chain exceeded65 bounded entries".into());
        }
        let bytes = projection
            .breadcrumbs()
            .iter()
            .try_fold(0usize, |sum, crumb| {
                if crumb.label().len() > 4096 {
                    None
                } else {
                    sum.checked_add(crumb.label().len())
                }
            })
            .ok_or("Original folder label preflight exceeded raw limit")?;
        if bytes > 128 * 1024 {
            return Err("Original folder command labels exceed128KiB; no cloned catalogue".into());
        }
        for (index, crumb) in projection.breadcrumbs().iter().enumerate() {
            let intent = TreeFolderIntentV2::capture(projection.clone(), index)?;
            let label = short(crumb.label());
            let action = if app.loading[View::Repository.index()] {
                Action::Explain(
                    "Original folder read pending; cancel/wait before navigating".into(),
                )
            } else {
                Action::RepositoryRow(RowAction::Tree(crumb.action().clone()))
            };
            entries.push(entry(
                &format!("repository.tree.folder.{index}"),
                format!("Open original folder {label}"),
                action,
            ));
            entries.push(entry(
                &format!("repository.tree.folder-detail.{index}"),
                format!("Inspect exact folder {label}"),
                if app.loading[View::Repository.index()] {
                    Action::Explain(
                        "Wait/cancel the current original read before folder detail".into(),
                    )
                } else {
                    Action::OriginalTreeViewportV2(TreeViewportActionV2::InspectFolder(intent))
                },
            ));
        }
    }
    // At most4+2*65, not entries for the50k inventory. IDs remain original
    // catalogue indices; displayed short labels are never target parsing.
    if entries.len() > 134 {
        return Err("Original tree contextual entry count exceeded finite bound".into());
    }
    Ok(entries)
}
fn short(value: &str) -> String {
    if value.len() <= 180 {
        return value.into();
    }
    let mut end = 76;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    let mut start = value.len() - 76;
    while !value.is_char_boundary(start) {
        start += 1;
    }
    // Preserve the genuine observed suffix for discovery of deep folders.
    // This is presentation/search only; the action still owns the original
    // member ticket and exact full-path detail, never this clipped label.
    format!("{}…{} [name clipped]", &value[..end], &value[start..])
}
fn entry(id: &str, label: String, action: Action) -> PaletteEntry {
    PaletteEntry {
        id: id.into(),
        label,
        keywords: vec![
            "git".into(),
            "original".into(),
            "tree".into(),
            "folder".into(),
            "path".into(),
        ],
        action,
    }
}

#[cfg(test)]
#[path = "repository_tree_palette_v2_tests.rs"]
mod tests;
