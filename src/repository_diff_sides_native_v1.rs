//! Small same-action App/controller hook; parent fields/matches remain unapplied.
use crate::{
    app::{Action, App, View},
    repository_diff_sides_v1::{
        OriginalDiffActionV1 as DiffAction, OriginalDiffEffectV1 as Effect, OriginalDiffPickerV1,
    },
    repository_diff_sides_worker_v1::OriginalDiffWorkV1,
    workspace::OriginalDiffReturnTicketV1,
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use std::sync::Arc;

pub fn retire(app: &mut App) {
    if app.original_diff_picker_v1.is_some() || app.original_diff_work_v1.is_some() {
        app.original_diff_cancel_v1 = true;
    }
    app.original_diff_picker_v1 = None;
    app.original_diff_work_v1 = None;
    app.original_diff_layout_v1.clear();
    app.hits.clear();
}
pub fn synchronize_source(app: &mut App) {
    if app.original_diff_picker_v1.as_ref().is_some_and(|picker| {
        app.view != View::Repository
            || app.remote_repository.is_some()
            || app
                .workspace
                .as_ref()
                .is_none_or(|workspace| !picker.owns(workspace))
    }) {
        retire(app);
    }
}
pub fn dispatch(app: &mut App, action: DiffAction) {
    synchronize_source(app);
    if action == DiffAction::Open {
        let intent = if app.view != View::Repository
            || app.remote_repository.is_some()
            || app.loading[View::Repository.index()]
        {
            Err(
                "Choose a loaded original saved comparison first; wait or cancel the active read"
                    .into(),
            )
        } else {
            app.workspace
                .as_ref()
                .ok_or_else(|| "Original comparison source unavailable".to_owned())
                .and_then(crate::git::OriginalComparisonIntentV1::capture)
        };
        match intent {
            Ok(intent) => {
                let mut picker = OriginalDiffPickerV1::new(intent);
                match picker.request() {
                    Ok(request) => {
                        app.original_diff_picker_v1 = Some(picker);
                        app.original_diff_work_v1 =
                            Some(Arc::new(OriginalDiffWorkV1::Query(request)));
                        app.original_diff_layout_v1.clear();
                        app.hits.clear();
                    }
                    Err(reason) => app.status = reason,
                }
            }
            Err(reason) => app.dispatch(Action::Explain(reason)),
        }
        return;
    }
    let Some(picker) = app.original_diff_picker_v1.as_mut() else {
        return;
    };
    match picker.apply(action) {
        Ok(Effect::None) => {}
        Ok(Effect::Query(request)) => {
            app.original_diff_work_v1 = Some(Arc::new(OriginalDiffWorkV1::Query(request)));
            app.original_diff_layout_v1.clear();
            app.hits.clear();
        }
        Ok(Effect::Side(ticket)) => {
            app.original_diff_work_v1 = Some(Arc::new(OriginalDiffWorkV1::Side(ticket)));
            app.original_diff_layout_v1.clear();
            app.hits.clear();
        }
        Ok(Effect::Explain(reason)) => app.dispatch(Action::Explain(reason)),
        Ok(Effect::Close) => {
            retire(app);
            app.status = "Original comparison file picker closed • no repository changes".into();
        }
        Ok(Effect::Quit) => app.dispatch(Action::Quit),
        Err(reason) => {
            picker.status = if reason.len() <= 4096 {
                format!(
                    "Error: {}",
                    crate::providers::runner::sanitize_terminal(&reason)
                )
            } else {
                "Error: Original comparison action unavailable; diagnostic omitted".into()
            };
            app.status = reason;
        }
    }
}
pub fn back(app: &mut App, ticket: OriginalDiffReturnTicketV1) {
    if app.loading[View::Repository.index()] {
        app.status = "Wait for the current original read, or cancel it first".into();
        return;
    }
    if app.view != View::Repository
        || app.remote_repository.is_some()
        || app
            .workspace
            .as_ref()
            .is_none_or(|workspace| !ticket.side().owns(workspace))
    {
        return;
    }
    retire(app);
    app.original_diff_work_v1 = Some(Arc::new(OriginalDiffWorkV1::Back(ticket.side().clone())));
}
pub fn capture(app: &mut App, event: Event) -> bool {
    synchronize_source(app);
    let Some(picker) = app.original_diff_picker_v1.as_ref() else {
        return false;
    };
    if matches!(&event,Event::Key(key) if key.kind==KeyEventKind::Release) {
        return true;
    }
    if matches!(&event,Event::Key(key) if key.code==KeyCode::Char('c')&&key.modifiers.contains(KeyModifiers::CONTROL))
    {
        app.dispatch(Action::Quit);
        return true;
    }
    let action = match event {
        Event::Key(key) => app
            .original_diff_layout_v1
            .key(picker, key.code, key.modifiers),
        Event::Mouse(mouse) => {
            app.original_diff_layout_v1
                .mouse(mouse.kind, mouse.column, mouse.row)
        }
        Event::Paste(value) => Some(DiffAction::Paste(value)),
        Event::Resize(_, _) => {
            app.original_diff_layout_v1.clear();
            app.hits.clear();
            None
        }
        _ => None,
    };
    if let Some(action) = action {
        dispatch(app, action);
    }
    true
}
pub fn draw(frame: &mut ratatui::Frame, app: &mut App, area: ratatui::layout::Rect) -> bool {
    synchronize_source(app);
    let theme = app.theme();
    let Some(picker) = app.original_diff_picker_v1.as_mut() else {
        return false;
    };
    app.original_diff_layout_v1.draw(frame, picker, area, theme);
    true
}
