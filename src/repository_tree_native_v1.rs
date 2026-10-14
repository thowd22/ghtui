//! Small original-tree controller binding; no provider/Git/SQL in frontend.
//! App/Action/module fields are an UNAPPLIED integration artifact.
use crate::{
    app::{Action, App, View},
    repository_tree_jump_v1::{
        OriginalTreeJumpActionV1 as TreeAction, OriginalTreeJumpEffectV1 as Effect,
        OriginalTreeJumpV1,
    },
    repository_tree_worker_v1::OriginalTreeWorkV1,
    workspace::{OriginalTreeNavigationActionV1, RowAction},
};
use crossterm::event::Event;
use std::sync::Arc;

pub fn retire(app: &mut App) {
    crate::repository_tree_viewport_native_v2::retire(app);
    if app.original_tree_jump_v1.is_some() || app.original_tree_work_v1.is_some() {
        app.original_tree_cancel_v1 = true;
    }
    app.original_tree_jump_v1 = None;
    app.original_tree_work_v1 = None;
    app.original_tree_layout_v1.clear();
    app.hits.clear();
}
pub fn synchronize_source(app: &mut App) {
    if app.original_tree_jump_v1.as_ref().is_some_and(|jump| {
        app.view != View::Repository
            || app.remote_repository.is_some()
            || app
                .workspace
                .as_ref()
                .is_none_or(|workspace| !jump.owns(workspace))
    }) {
        retire(app);
    }
}
pub fn dispatch(app: &mut App, action: TreeAction) {
    synchronize_source(app);
    if action == TreeAction::Open {
        let intent = if app.loading[View::Repository.index()] {
            Err("Wait for the current original repository read or cancel it first".into())
        } else {
            app.workspace
                .as_ref()
                .ok_or_else(|| "Open an original saved Tree first".to_owned())
                .and_then(crate::git::OriginalTreeIntentV1::capture)
        };
        match intent {
            Ok(intent) => {
                let mut jump = OriginalTreeJumpV1::new(intent);
                match jump.request() {
                    Ok(request) => {
                        app.original_tree_work_v1 =
                            Some(Arc::new(OriginalTreeWorkV1::Query(request)));
                        app.original_tree_jump_v1 = Some(jump);
                        app.original_tree_layout_v1.clear();
                        app.hits.clear();
                    }
                    Err(reason) => app.status = reason,
                }
            }
            Err(reason) => app.dispatch(Action::Explain(reason)),
        }
        return;
    }
    let Some(jump) = app.original_tree_jump_v1.as_mut() else {
        return;
    };
    match jump.apply(action) {
        Ok(Effect::None) => {}
        Ok(Effect::Request(request)) => {
            crate::repository_tree_viewport_native_v2::retire(app);
            app.original_tree_work_v1 = Some(Arc::new(OriginalTreeWorkV1::Query(request)));
            app.original_tree_layout_v1.clear();
            app.hits.clear();
        }
        Ok(Effect::Open(ticket)) => {
            crate::repository_tree_viewport_native_v2::retire(app);
            app.original_tree_work_v1 = Some(Arc::new(OriginalTreeWorkV1::Jump(ticket)));
            app.original_tree_layout_v1.clear();
            app.hits.clear();
        }
        Ok(Effect::Close) => {
            retire(app);
            app.status = "Original saved-path picker closed • no repository changes".into();
        }
        Err(reason) => app.status = reason,
    }
    crate::repository_tree_viewport_native_v2::queue_member(app);
}
pub fn row(app: &mut App, action: OriginalTreeNavigationActionV1) {
    if app.loading[View::Repository.index()] {
        app.status = "Wait for the current original read, or cancel it first".into();
        return;
    }
    let Some(projection) = app.workspace.as_ref().and_then(|workspace| {
        workspace
            .tree_navigation_v1
            .as_ref()
            .filter(|projection| projection.visible_for(workspace))
    }) else {
        return;
    };
    app.original_tree_work_v1 = Some(Arc::new(OriginalTreeWorkV1::Navigate {
        projection: projection.clone(),
        action,
    }));
    app.hits.clear();
}
/// Owned modal captures EVERY event, including unknown keys/outside clicks,
/// so no background Repository/global/mutation control receives fall-through.
pub fn capture(app: &mut App, event: Event) -> bool {
    synchronize_source(app);
    crate::repository_tree_viewport_ui_v2::event(app, event)
}
pub fn draw(frame: &mut ratatui::Frame, app: &mut App, area: ratatui::layout::Rect) -> bool {
    synchronize_source(app);
    crate::repository_tree_viewport_ui_v2::draw(frame, app, area)
}
/// Pure list/breadcrumb input dispatch uses exact prepared original actions.
pub fn selected_row(app: &App) -> Option<OriginalTreeNavigationActionV1> {
    let workspace = app.workspace.as_ref()?;
    let row = workspace.display_rows_v1().get(app.selected)?;
    match row.action.as_ref()? {
        RowAction::Tree(action) => Some(action.clone()),
        _ => None,
    }
}
