//! Same closed actions for keyboard/mouse; all projection changes use B's worker.
use crate::{
    app::{Action, App, View},
    repository_diff_sides_worker_v1::OriginalDiffWorkV1,
    repository_native_diff_ui_v1::NativeDiffActionV1 as A,
    repository_native_diff_v1::{NativeDiffOptionChangeV1, NativeDiffStateV1},
};
use crossterm::event::{Event, KeyEventKind};
use std::sync::Arc;

pub fn retire(app: &mut App) {
    if app.native_diff_v1.take().is_some() {
        app.original_diff_cancel_v1 = true;
        app.original_diff_work_v1 = None;
    }
    app.native_diff_layout_v1 = Default::default();
    app.hits.clear();
}
pub fn synchronize(app: &mut App) {
    if app.native_diff_v1.as_ref().is_some_and(|state| {
        app.view != View::Repository
            || app.remote_repository.is_some()
            || app
                .workspace
                .as_ref()
                .is_none_or(|workspace| !state.owns(workspace))
    }) {
        retire(app);
    }
}
fn queue(app: &mut App) {
    let Some(state) = app.native_diff_v1.as_mut() else {
        return;
    };
    match state.request() {
        Ok(request) => {
            app.original_diff_work_v1 = Some(Arc::new(OriginalDiffWorkV1::Native(request)))
        }
        Err(reason) => {
            state.status = reason;
            app.original_diff_work_v1 = None;
            app.original_diff_cancel_v1 = true;
        }
    }
    app.native_diff_layout_v1.clear();
    app.hits.clear();
}
fn visible(app: &mut App) {
    let height = app.native_diff_layout_v1.body_height();
    if let Some(state) = app.native_diff_v1.as_mut() {
        if state.selected < state.scroll {
            state.scroll = state.selected;
        } else if state.selected >= state.scroll.saturating_add(height) {
            state.scroll = state.selected.saturating_add(1).saturating_sub(height);
        }
    }
}
pub fn dispatch(app: &mut App, action: A) {
    synchronize(app);
    if action == A::Open {
        if app.view != View::Repository
            || app.remote_repository.is_some()
            || app.loading[View::Repository.index()]
            || app.repository_load_requested
            || app.repository_action.is_some()
        {
            app.dispatch(Action::Explain(
                "Choose a loaded local saved comparison first; wait or cancel its active read"
                    .into(),
            ));
            return;
        }
        let opened = app
            .workspace
            .as_ref()
            .ok_or_else(|| "Native diff source is unavailable".to_owned())
            .and_then(|workspace| {
                NativeDiffStateV1::open(
                    workspace,
                    app.original_tree_viewport_v2
                        .width
                        .saturating_sub(2)
                        .min(512),
                )
            });
        match opened {
            Ok(state) => {
                crate::repository_tree_native_v1::retire(app);
                crate::repository_diff_sides_native_v1::retire(app);
                retire(app);
                app.native_diff_v1 = Some(state);
                app.native_diff_layout_v1 = Default::default();
                queue(app);
            }
            Err(reason) => app.dispatch(Action::Explain(reason)),
        }
        return;
    }
    if action == A::Quit {
        app.dispatch(Action::Quit);
        return;
    }
    if action == A::Close {
        retire(app);
        app.status = "Native saved diff closed • no repository changes".into();
        return;
    }
    if app.native_diff_v1.is_none() {
        return;
    }
    if action == A::Source {
        app.native_diff_layout_v1.clear();
        app.dispatch(Action::RepositorySource);
        return;
    }
    if action == A::Controls {
        app.native_diff_layout_v1.controls = !app.native_diff_layout_v1.controls;
        app.native_diff_layout_v1.focus = None;
        app.native_diff_layout_v1.clear();
        return;
    }
    if matches!(action, A::FocusQuery | A::FocusBody) {
        app.native_diff_layout_v1.query_focused = action == A::FocusQuery;
        app.native_diff_layout_v1.controls = false;
        app.native_diff_layout_v1.focus = None;
        return;
    }
    if let A::FocusNext(back) = action {
        app.native_diff_layout_v1.cycle(back);
        return;
    }
    if action == A::Activate {
        if let Some(action) = app.native_diff_layout_v1.activated() {
            dispatch(app, action);
        }
        return;
    }
    let state = app.native_diff_v1.as_mut().expect("capturing native state");
    let mut prepare = false;
    let mut error = None;
    match action {
        A::QueryClick(column)=>{app.native_diff_layout_v1.query_focused=true;app.native_diff_layout_v1.focus=None;state.query.click(0,column,false);}
        A::Insert(value)=>{
            // Reject before TextBuffer normalization/allocation or cloning a
            // pasted payload. The query is a selected-file patch scope only.
            let selection=state.query.selection().map_or(0,|range|range.len());
            if value.len()>256||state.query.text.len()-selection+value.len()>256||value.chars().any(char::is_control){error=Some("Native patch query is limited to256 bytes; control bytes are refused".into());}
            else{match state.query.insert(&value){Ok(())=>prepare=true,Err(reason)=>error=Some(reason)}}
        }
        A::Backspace=>{state.query.delete_backward();prepare=true;}
        A::Delete=>{state.query.delete_forward();prepare=true;}
        A::SelectAll=>state.query.select_all(),A::ClearQuery=>{state.query.clear();prepare=true;}
        A::Cursor{right,select}=>state.query.horizontal(right,select),A::Edge{end,select}=>state.query.line_edge(end,true,select),
        A::Regex=>{state.regex = !state.regex;prepare=true;}
        A::Option(change)=>{match state.option(change){Ok(())=>prepare=true,Err(reason)=>error=Some(reason)}}
        A::File(delta)=>{
            if let Some(source)=state.source(){if source.members().is_empty(){error=Some("No admitted changed files in this original comparison; no file can be opened".into());}else{let next=state.file.saturating_add_signed(delta as isize).min(source.members().len().saturating_sub(1));if next!=state.file{state.file=next;state.selected=0;state.scroll=0;prepare=true;}}}
            else{error=Some("Wait for the original changed-file catalogue".into());}
        }
        A::Point{ticket,row,right}=>{if !state.current_frame().is_some_and(|frame|frame.accepts(&ticket)&&frame.ticket(row,right).as_ref()==Some(&ticket)){return;}state.selected=row;state.right=right;app.native_diff_layout_v1.query_focused=false;app.native_diff_layout_v1.focus=None;}
        A::Drag{first,last}=>match state.select_between(first,last){Ok(())=>prepare=true,Err(reason)=>error=Some(reason)},
        A::Move{delta,extend}=>{
            if let Some(frame)=state.current_frame(){let next=state.selected.saturating_add_signed(delta as isize).min(frame.rows().len().saturating_sub(1));let ticket=frame.ticket(next,state.right);state.selected=next;if extend {if let Some(ticket)=ticket{match state.select(ticket,true){Ok(())=>prepare=true,Err(reason)=>error=Some(reason)}}}}
        }
        A::First=>state.selected=0,A::Last=>{if let Some(frame)=state.current_frame(){state.selected=frame.rows().len().saturating_sub(1);}}
        A::Horizontal(delta)=>state.horizontal=(i32::from(state.horizontal)+delta).clamp(0,4096) as u16,
        A::Scroll(delta)=>{if let Some(frame)=state.frame(){state.scroll=state.scroll.saturating_add_signed(delta as isize).min(frame.rows().len().saturating_sub(1));}}
        A::Hunk(delta)=>{
            if let Some(frame)=state.current_frame(){let current=frame.ticket(state.selected,state.right).map(|ticket|ticket.key().hunk());let index=current.map_or(0,|current|current.saturating_add_signed(delta as isize));if let Some(rows)=frame.hunks().get(index).and_then(|hunk|hunk.rows()){state.selected=rows.start;}else{error=Some("Requested hunk is hidden or outside this selected-file patch".into());}}
        }
        A::Match(delta)=>{
            if let Some(frame)=state.current_frame(){let matches=frame.matches();let coordinate=frame.ticket(state.selected,state.right).map(|ticket|(ticket.key().hunk(),ticket.key().line()));let index=coordinate.map_or_else(||if delta>0{0}else{matches.len().saturating_sub(1)},|coordinate|if delta>0{matches.partition_point(|m|(m.key().hunk(),m.key().line())<=coordinate)}else{matches.partition_point(|m|(m.key().hunk(),m.key().line())<coordinate).saturating_sub(1)});if let Some(found)=matches.get(index).or_else(||matches.first()){if let Some(row)=frame.first_matching_line_row(found.key()){state.selected=row;}else{error=Some("Matching original line is hidden by context/generated/focused-side options; no neighbouring row was selected".into());}}else{error=Some("No matches in the selected-file patch; full-side content was not searched".into());}}
        }
        A::Select=>{if let Some(ticket)=state.current_frame().and_then(|frame|frame.ticket(state.selected,state.right)){match state.select(ticket,false){Ok(())=>prepare=true,Err(reason)=>error=Some(reason)}}else{error=Some("Choose an original changed line, not a header or blank cell".into());}}
        A::SelectHunk=>{
            let hunk=state.current_frame().and_then(|frame|frame.ticket(state.selected,state.right)).map(|ticket|ticket.key().hunk());
            if let Some(hunk)=hunk{match state.select_hunk(hunk){Ok(())=>prepare=true,Err(reason)=>error=Some(reason)}}else{error=Some("Choose a genuine changed-line row before selecting its hunk".into());}
        }
        A::ClearRange=>{state.clear_selection();prepare=true;}
        A::Stage=>state.status="Stage disabled: this is a saved comparison; no current index/worktree authority or write was created".into(),
        _=>{}
    }
    if let Some(reason) = error {
        state.status = reason;
    }
    if prepare {
        queue(app);
    } else {
        visible(app);
    }
}
pub fn capture(app: &mut App, event: &Event) -> bool {
    synchronize(app);
    if app.native_diff_v1.is_none() || app.overlay.is_some() {
        return false;
    }
    match event {
        Event::Resize(width, _) => {
            let width = width.saturating_sub(2).min(512);
            if app
                .native_diff_v1
                .as_ref()
                .is_some_and(|state| state.width != width)
            {
                app.native_diff_v1.as_mut().expect("native resize").width = width;
                queue(app);
            } else {
                app.native_diff_layout_v1.clear();
            }
            return true;
        }
        Event::Key(key) if key.kind == KeyEventKind::Release => return true,
        Event::Key(key) => {
            if key.code == crossterm::event::KeyCode::Char('g')
                && !app.native_diff_layout_v1.query_focused
            {
                let file = app.native_diff_v1.as_ref().expect("native control").file;
                dispatch(
                    app,
                    A::Option(NativeDiffOptionChangeV1::MarkGenerated(file)),
                );
                return true;
            }
            if let Some(action) = app.native_diff_layout_v1.key(key.code, key.modifiers) {
                dispatch(app, action);
            }
        }
        Event::Mouse(mouse) => {
            if let Some(action) =
                app.native_diff_layout_v1
                    .mouse(mouse.kind, mouse.column, mouse.row)
            {
                dispatch(app, action);
            }
        }
        Event::Paste(value) => {
            if app.native_diff_layout_v1.query_focused {
                if value.len() <= 256 {
                    dispatch(app, A::Insert(value.clone()));
                } else {
                    app.native_diff_v1.as_mut().expect("native query").status =
                        "Native patch query is limited to256 bytes; paste refused without cloning"
                            .into();
                }
            }
        }
        _ => {}
    }
    true
}
pub fn draw(frame: &mut ratatui::Frame, app: &mut App, area: ratatui::layout::Rect) -> bool {
    synchronize(app);
    let theme = app.theme();
    let Some(state) = app.native_diff_v1.as_mut() else {
        return false;
    };
    app.native_diff_layout_v1.draw(frame, state, area, theme);
    true
}
