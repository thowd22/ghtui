//! Owned remote browser: Catalogue navigation is not scope activation. Every
//! completion, including error/panic, is bound before private presentation.
use crate::{
    app::{App, View},
    providers::{
        gh::{GithubProvider, RemoteRepositoryLimitsV1, RemoteRepositoryRequestV1},
        runner::CancellationToken,
    },
    remote_repository_v1::{
        self as model, RemoteBrowseActionV1, RemoteBrowsePageV1, RemoteHandoffPreviewV1,
        RemoteRepositoryOwnerV1, RemoteRepositoryPreparedV1, RemoteRepositoryWorkspaceV1,
    },
    runtime::PreparedView,
    storage::CatalogueIntentSource,
};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

pub struct RemoteRepositoryRequestOwnerV1 {
    owner: Arc<RemoteRepositoryOwnerV1>,
    source: Arc<CatalogueIntentSource>,
    generation: u64,
    deadline: Instant,
}
impl RemoteRepositoryRequestOwnerV1 {
    pub fn expired(&self) -> bool {
        Instant::now() >= self.deadline
    }
    pub fn host(&self) -> &str {
        &self.source.scope().host
    }
    pub fn owns(&self, app: &App) -> bool {
        app.remote_repository.as_ref().is_some_and(|state| {
            state
                .pending
                .as_ref()
                .is_some_and(|current| std::ptr::eq(self, current.as_ref()))
                && Arc::ptr_eq(&state.owner, &self.owner)
                && Arc::ptr_eq(&state.source, &self.source)
                && state.generation == self.generation
        })
    }
}
pub struct RemoteRepositoryNativeV1 {
    source: Arc<CatalogueIntentSource>,
    owner: Arc<RemoteRepositoryOwnerV1>,
    generation: u64,
    workspace: Option<RemoteRepositoryWorkspaceV1>,
    pending: Option<Arc<RemoteRepositoryRequestOwnerV1>>,
    request: Option<RemoteRepositoryWorkV1>,
    dialog: Option<RemoteDialogV1>,
    limits: RemoteRepositoryLimitsV1,
    return_view: View,
    cancel_requested: bool,
    hits: Vec<(ratatui::layout::Rect, RemoteBrowseActionV1)>,
    dialog_focus: usize,
    editor_area: ratatui::layout::Rect,
    input_error: Option<&'static str>,
}
enum RemoteDialogV1 {
    Commit(crate::editor::TextBuffer),
    Notice {
        lines: Arc<[String]>,
        scroll: usize,
    },
    Handoff {
        original: Arc<RemoteHandoffPreviewV1>,
        scroll: usize,
    },
}
pub struct RemoteRepositoryWorkV1 {
    pub owner: Arc<RemoteRepositoryRequestOwnerV1>,
    kind: WorkKindV1,
    limits: RemoteRepositoryLimitsV1,
}
enum WorkKindV1 {
    Read(RemoteRepositoryRequestV1),
    Preview(Arc<crate::providers::gh::RemoteFileV1>),
    Confirm(Arc<RemoteHandoffPreviewV1>),
}
pub enum RemoteRepositoryCompletionV1 {
    Read(RemoteRepositoryPreparedV1),
    Preview(Result<RemoteHandoffPreviewV1, String>),
    Local(Result<model::RemoteLocalHandoffV1, String>),
}
pub fn initial(
    original: Arc<CatalogueIntentSource>,
    index: usize,
    gh: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
) -> Result<RemoteRepositoryNativeV1, String> {
    let owner = RemoteRepositoryOwnerV1::from_row(original.source().clone(), index)?;
    let result = model::read_remote_repository_v1(
        model::RemoteRepositoryEnvironmentV1 {
            provider: gh,
            database,
        },
        original.clone(),
        owner.clone(),
        1,
        RemoteRepositoryRequestV1::DefaultTree,
        &RemoteRepositoryLimitsV1::default(),
        cancel,
    );
    let workspace = match result {
        RemoteRepositoryPreparedV1::Ready { workspace, .. } => Some(*workspace),
        RemoteRepositoryPreparedV1::Hidden {
            scope_unavailable: false,
            ..
        } => None,
        _ => return Err("Remote repository unavailable • private tree/content hidden".into()),
    };
    Ok(RemoteRepositoryNativeV1 {
        source: original,
        owner,
        generation: 1,
        workspace,
        pending: None,
        request: None,
        dialog: None,
        limits: Default::default(),
        return_view: View::Repository,
        cancel_requested: false,
        hits: vec![],
        dialog_focus: 0,
        editor_area: Default::default(),
        input_error: None,
    })
}
pub fn install_initial(app: &mut App, mut state: RemoteRepositoryNativeV1) {
    state.return_view = app.view;
    app.catalogue = None;
    app.overlay = None;
    app.hits.clear();
    app.avatar_states.clear();
    app.local_actor_cancel_requested |= app.local_commit_actors.is_pending();
    app.local_commit_actors.retire_private();
    app.local_actor_requested = false;
    app.local_actor_layout.clear();
    app.remote_repository = Some(state);
    // Do not change app.view/workspace/form/search/remote_scope/saved intent.
}
pub fn retire(app: &mut App) {
    app.remote_repository = None;
    app.remote_repository_cancel_requested = true;
    app.hits.clear();
}
pub fn take_request(app: &mut App) -> Option<RemoteRepositoryWorkV1> {
    app.remote_repository.as_mut()?.request.take()
}
pub fn take_cancel(app: &mut App) -> bool {
    let pending = app
        .remote_repository
        .as_mut()
        .is_some_and(|state| std::mem::take(&mut state.cancel_requested));
    std::mem::take(&mut app.remote_repository_cancel_requested) || pending
}
pub fn execute(
    work: RemoteRepositoryWorkV1,
    gh: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    let owner = &work.owner;
    if cancel.is_cancelled() || Instant::now() >= owner.deadline {
        return Err("Original remote read expired/cancelled before worker".into());
    }
    let gh = gh.clone().with_catalogue_read_deadline(owner.deadline);
    let result = match work.kind {
        WorkKindV1::Read(request) => {
            RemoteRepositoryCompletionV1::Read(model::read_remote_repository_v1(
                model::RemoteRepositoryEnvironmentV1 {
                    provider: &gh,
                    database,
                },
                owner.source.clone(),
                owner.owner.clone(),
                owner.generation,
                request,
                &work.limits,
                cancel,
            ))
        }
        WorkKindV1::Preview(file) => {
            RemoteRepositoryCompletionV1::Preview(model::preview_remote_local_handoff_v1(
                model::RemoteRepositoryEnvironmentV1 {
                    provider: &gh,
                    database,
                },
                owner.source.clone(),
                owner.owner.clone(),
                owner.generation,
                file,
                &work.limits,
                cancel,
            ))
        }
        WorkKindV1::Confirm(original) => {
            RemoteRepositoryCompletionV1::Local(model::confirm_remote_local_handoff_v1(
                &gh,
                database,
                &original,
                owner.generation,
                &work.limits,
                cancel,
            ))
        }
    };
    if cancel.is_cancelled() || Instant::now() >= owner.deadline {
        return Err("Original remote deadline expired after preparation; no late page".into());
    }
    Ok(PreparedView {
        remote_repository: Some(result),
        ..Default::default()
    })
}
pub fn accept(app: &mut App, result: RemoteRepositoryCompletionV1) -> bool {
    let Some(current) = app.remote_repository.as_ref() else {
        return false;
    };
    let matches = match &result {
        RemoteRepositoryCompletionV1::Read(value) => {
            Arc::ptr_eq(value.owner(), &current.owner)
                && Arc::ptr_eq(value.source(), &current.source)
                && value.generation() == current.generation
        }
        RemoteRepositoryCompletionV1::Preview(Ok(value)) => {
            Arc::ptr_eq(value.owner(), &current.owner)
                && Arc::ptr_eq(value.source(), &current.source)
                && value.generation() == current.generation
        }
        RemoteRepositoryCompletionV1::Local(Ok(value)) => {
            Arc::ptr_eq(&value.owner, &current.owner)
                && Arc::ptr_eq(&value.source, &current.source)
                && value.generation == current.generation
                && value.identity.host == current.source.scope().host
                && value.identity.account_id == current.source.source().identity().account_id
        }
        _ => true,
    };
    if !matches {
        return false;
    }
    if matches!(
        &result,
        RemoteRepositoryCompletionV1::Read(RemoteRepositoryPreparedV1::Hidden {
            scope_unavailable: true,
            ..
        }) | RemoteRepositoryCompletionV1::Preview(Err(_))
            | RemoteRepositoryCompletionV1::Local(Err(_))
    ) {
        crate::repository_catalogue_native::privacy(app);
        crate::repository_catalogue_native::retire_remote_views(app);
        app.status =
            "Remote access unavailable • previous private pages hidden • local Git kept".into();
        return true;
    }
    let Some(state) = app.remote_repository.as_mut() else {
        return false;
    };
    // Request/Completion ownership was checked by Background BEFORE this call.
    state.pending = None;
    match result {
        RemoteRepositoryCompletionV1::Read(RemoteRepositoryPreparedV1::Ready {
            owner,
            source,
            generation,
            workspace,
        }) => {
            if !Arc::ptr_eq(&owner, &state.owner)
                || !Arc::ptr_eq(&source, &state.source)
                || generation != state.generation
            {
                return false;
            }
            state.workspace = Some(*workspace);
            state.dialog = None;
        }
        RemoteRepositoryCompletionV1::Preview(Ok(original)) => {
            if !Arc::ptr_eq(original.owner(), &state.owner)
                || !Arc::ptr_eq(original.source(), &state.source)
                || original.generation() != state.generation
            {
                return false;
            }
            state.dialog = Some(RemoteDialogV1::Handoff {
                original: Arc::new(original),
                scroll: 0,
            });
            if let Some(ws) = &mut state.workspace {
                ws.busy = false;
            }
        }
        RemoteRepositoryCompletionV1::Local(Ok(handoff)) => {
            if !Arc::ptr_eq(&handoff.owner, &state.owner)
                || !Arc::ptr_eq(&handoff.source, &state.source)
                || handoff.generation != state.generation
            {
                return false;
            }
            app.workspace = Some(*handoff.workspace);
            app.repository_source_identity = Some(handoff.repository_source_identity);
            app.repository_mode = crate::workspace::RepositoryMode::Tree;
            app.view = View::Repository;
            app.context = "Matching local snapshot bytes • not verified remote association".into();
            app.remote_repository = None;
            app.avatar_states.clear();
            app.hits.clear();
            app.status =
                "Exact saved commit/path retained • no checkout or persistent association".into();
            // Explicit content handoff does not discard a local form or mutate SQL.
        }
        RemoteRepositoryCompletionV1::Read(RemoteRepositoryPreparedV1::Hidden { .. })
        | RemoteRepositoryCompletionV1::Preview(Err(_))
        | RemoteRepositoryCompletionV1::Local(Err(_)) => {
            state.workspace = None;
            state.dialog=Some(RemoteDialogV1::Notice{lines:Arc::from(["Remote read unavailable • previous private tree/content hidden".into(),"Original source/current access must be loaded again; no implicit default/local HEAD fallback".into()]),scroll:0});
            app.avatar_states.clear();
            app.hits.clear();
        }
    }
    true
}
pub fn worker_failed(app: &mut App) {
    let _ = accept(
        app,
        RemoteRepositoryCompletionV1::Preview(Err(String::new())),
    );
}
fn queue(state: &mut RemoteRepositoryNativeV1, kind: WorkKindV1) {
    if state.pending.is_some() {
        return;
    }
    state.generation = state.generation.saturating_add(1);
    let owner = Arc::new(RemoteRepositoryRequestOwnerV1 {
        owner: state.owner.clone(),
        source: state.source.clone(),
        generation: state.generation,
        deadline: Instant::now() + state.limits.max_duration.min(Duration::from_secs(30)),
    });
    state.pending = Some(owner.clone());
    state.request = Some(RemoteRepositoryWorkV1 {
        owner,
        kind,
        limits: state.limits.clone(),
    });
    if let Some(ws) = &mut state.workspace {
        ws.busy = true;
        ws.clear_layout();
    }
    state.hits.clear();
}
pub fn dispatch(app: &mut App, action: RemoteBrowseActionV1) {
    let Some(state) = &mut app.remote_repository else {
        return;
    };
    if matches!(action, RemoteBrowseActionV1::Back) {
        if state.pending.take().is_some() {
            state.cancel_requested = true;
            app.remote_repository_cancel_requested = true;
            state.request = None;
            state.workspace = None;
        }
        if state.dialog.take().is_some() {
            return;
        }
        app.view = state.return_view;
        app.remote_repository = None;
        app.hits.clear();
        return;
    }
    if state.pending.is_some() {
        return;
    }
    if let Some(dialog) = &mut state.dialog {
        match dialog {
            RemoteDialogV1::Commit(editor) => match action {
                RemoteBrowseActionV1::CommitInsert(text) => {
                    let selected = editor.selection().map_or(0, |range| range.len());
                    if text.bytes().all(|b| b.is_ascii_hexdigit())
                        && editor.text.len() - selected + text.len() <= 64
                    {
                        let _ = editor.insert(&text);
                        state.input_error = None;
                    }
                }
                RemoteBrowseActionV1::CommitBackspace => editor.delete_backward(),
                RemoteBrowseActionV1::CommitLeft => editor.horizontal(false, false),
                RemoteBrowseActionV1::CommitRight => editor.horizontal(true, false),
                RemoteBrowseActionV1::CommitClear => editor.clear(),
                RemoteBrowseActionV1::CommitSelectAll => editor.select_all(),
                RemoteBrowseActionV1::CommitClick(column) => editor.click(0, column, false),
                RemoteBrowseActionV1::Commit => {
                    let id = editor.text.clone();
                    if !matches!(id.len(), 40 | 64)
                        || !id
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                        || id.bytes().all(|b| b == b'0')
                    {
                        state.input_error =
                            Some("Full40/64 lowercase saved SHA required • no request sent");
                        return;
                    }
                    queue(
                        state,
                        WorkKindV1::Read(RemoteRepositoryRequestV1::Commit(id)),
                    );
                }
                _ => {}
            },
            RemoteDialogV1::Handoff { original, scroll } => match action {
                RemoteBrowseActionV1::Open => {
                    let original = original.clone();
                    queue(state, WorkKindV1::Confirm(original));
                }
                RemoteBrowseActionV1::Move(delta) | RemoteBrowseActionV1::Page(delta) => {
                    *scroll = scroll.saturating_add_signed(delta as isize).min(4096)
                }
                _ => {}
            },
            RemoteDialogV1::Notice { scroll, .. } => match action {
                RemoteBrowseActionV1::Move(delta) | RemoteBrowseActionV1::Page(delta) => {
                    *scroll = scroll.saturating_add_signed(delta as isize).min(4096)
                }
                _ => {}
            },
        }
        return;
    }
    if state.workspace.is_none() {
        match action {
            RemoteBrowseActionV1::Branches => queue(
                state,
                WorkKindV1::Read(RemoteRepositoryRequestV1::References(
                    crate::providers::gh::RemoteReferenceKindV1::Branch,
                )),
            ),
            RemoteBrowseActionV1::Tags => queue(
                state,
                WorkKindV1::Read(RemoteRepositoryRequestV1::References(
                    crate::providers::gh::RemoteReferenceKindV1::Tag,
                )),
            ),
            RemoteBrowseActionV1::CommitFocus => {
                state.dialog = Some(RemoteDialogV1::Commit(Default::default()))
            }
            _ => {}
        }
        return;
    }
    let ws = state.workspace.as_mut().expect("checked actual workspace");
    match action {
        RemoteBrowseActionV1::Details => {
            let mut lines = ws.header.to_vec();
            lines.push(format!(
                "Limits: {} ref pages / {} refs / {} directory rows",
                state.limits.max_pages, state.limits.max_refs, state.limits.max_directory_rows
            ));
            lines.push(format!(
                "Blob {} bytes / raw aggregate {} bytes / original {} seconds",
                state.limits.max_blob_bytes,
                state.limits.max_response_bytes,
                state.limits.max_duration.as_secs()
            ));
            if let Some(actor) = &ws.actor {
                lines.push(format!(
                    "Repository owner: {} • not file author",
                    actor.label()
                ));
            }
            match &ws.page {
                RemoteBrowsePageV1::Directory(dir) => {
                    lines.push(format!("Exact commit: {}", dir.commit()));
                    lines.push(format!("Directory tree: {}", dir.tree()));
                    if let Some(row) = ws
                        .rows
                        .get(ws.selected)
                        .and_then(|row| dir.entries().get(row.index))
                    {
                        lines.push(format!(
                            "Exact name: {}",
                            crate::git::display_path_bytes(row.name().as_bytes())
                        ));
                        lines.push(format!("Object: {} • {}", row.oid(), row.kind().label()));
                    }
                }
                RemoteBrowsePageV1::File { original, .. } => {
                    lines.push(format!(
                        "Exact blob: {} • {} bytes",
                        original.entry().oid(),
                        original.bytes().len()
                    ));
                }
                RemoteBrowsePageV1::References(refs) => {
                    lines.push(
                        "Ref list is observed traversal, not atomic history or permission.".into(),
                    );
                    if let Some(row) = refs.entries().get(ws.selected) {
                        lines.push(format!(
                            "Exact name: {}",
                            crate::git::display_path_bytes(row.name().as_bytes())
                        ));
                        lines.push(format!("Observed commit: {}", row.commit()));
                    }
                }
            }
            state.dialog = Some(RemoteDialogV1::Notice {
                lines: lines.into(),
                scroll: 0,
            });
        }
        RemoteBrowseActionV1::CommitFocus => {
            state.dialog = Some(RemoteDialogV1::Commit(Default::default()));
        }
        RemoteBrowseActionV1::MappedLocal => {
            let selected = if let RemoteBrowsePageV1::File { original, .. } = &ws.page {
                Some(original.clone())
            } else {
                None
            };
            if let Some(original) = selected {
                queue(state, WorkKindV1::Preview(original));
            } else {
                state.dialog=Some(RemoteDialogV1::Notice{lines:Arc::from(["Open an exact verified file before local handoff; directory/reported entries are not content proof".into()]),scroll:0});
            }
        }
        RemoteBrowseActionV1::Up => {
            let original = match &ws.page {
                RemoteBrowsePageV1::File { original, .. } => Some(original.directory().clone()),
                RemoteBrowsePageV1::Directory(dir) => dir.parent().cloned(),
                _ => None,
            };
            if let Some(original) = original {
                queue(
                    state,
                    WorkKindV1::Read(RemoteRepositoryRequestV1::ReloadDirectory { original }),
                );
            } else {
                state.dialog = Some(RemoteDialogV1::Notice {
                    lines: Arc::from([
                        "Already at the snapshot root • no current HEAD guessed".into()
                    ]),
                    scroll: 0,
                });
            }
        }
        _ => match ws.request(&action) {
            Ok(Some(request)) => queue(state, WorkKindV1::Read(request)),
            Ok(None) => {
                let _ = ws.local(&action);
            }
            Err(reason) => {
                state.dialog = Some(RemoteDialogV1::Notice {
                    lines: Arc::from([reason]),
                    scroll: 0,
                })
            }
        },
    }
}

pub fn event(app: &mut App, event: crossterm::event::Event) {
    use crossterm::event::{
        Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
    };
    let Some(state) = &mut app.remote_repository else {
        return;
    };
    if matches!(event, Event::Resize(..)) {
        state.hits.clear();
        state.editor_area = Default::default();
        if let Some(ws) = &mut state.workspace {
            ws.clear_layout();
        }
        return;
    }
    let action = match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                app.quit = true;
                return;
            }
            if state.pending.is_some() {
                (key.code == KeyCode::Esc).then_some(RemoteBrowseActionV1::Back)
            } else if let Some(dialog) = &state.dialog {
                if matches!(key.code, KeyCode::Tab | KeyCode::BackTab) {
                    state.dialog_focus = 1 - state.dialog_focus.min(1);
                    return;
                }
                Some(match key.code {
                    KeyCode::Esc => RemoteBrowseActionV1::Back,
                    KeyCode::Enter => {
                        if state.dialog_focus == 1 {
                            RemoteBrowseActionV1::Back
                        } else {
                            match dialog {
                                RemoteDialogV1::Commit(_) => RemoteBrowseActionV1::Commit,
                                RemoteDialogV1::Handoff { .. } => RemoteBrowseActionV1::Open,
                                RemoteDialogV1::Notice { .. } => RemoteBrowseActionV1::Back,
                            }
                        }
                    }
                    KeyCode::PageUp => RemoteBrowseActionV1::Page(-8),
                    KeyCode::PageDown => RemoteBrowseActionV1::Page(8),
                    KeyCode::Up => RemoteBrowseActionV1::Move(-1),
                    KeyCode::Down => RemoteBrowseActionV1::Move(1),
                    KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        RemoteBrowseActionV1::CommitClear
                    }
                    KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        RemoteBrowseActionV1::CommitSelectAll
                    }
                    KeyCode::Char(c) if matches!(dialog, RemoteDialogV1::Commit(_)) => {
                        RemoteBrowseActionV1::CommitInsert(c.to_string())
                    }
                    KeyCode::Backspace => RemoteBrowseActionV1::CommitBackspace,
                    KeyCode::Left => RemoteBrowseActionV1::CommitLeft,
                    KeyCode::Right => RemoteBrowseActionV1::CommitRight,
                    _ => return,
                })
            } else {
                state
                    .workspace
                    .as_ref()
                    .and_then(|ws| ws.key(key))
                    .or(match key.code {
                        KeyCode::Esc => Some(RemoteBrowseActionV1::Back),
                        KeyCode::Char('B') => Some(RemoteBrowseActionV1::Branches),
                        KeyCode::Char('T') => Some(RemoteBrowseActionV1::Tags),
                        KeyCode::Char('C') => Some(RemoteBrowseActionV1::CommitFocus),
                        _ => None,
                    })
            }
        }
        Event::Paste(value) if matches!(state.dialog, Some(RemoteDialogV1::Commit(_))) => {
            Some(RemoteBrowseActionV1::CommitInsert(value))
        }
        Event::Mouse(mouse) => {
            if state.pending.is_some() {
                if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                    state.hits.iter().find_map(|(rect, action)| {
                        (*action == RemoteBrowseActionV1::Back
                            && rect
                                .contains(ratatui::layout::Position::new(mouse.column, mouse.row)))
                        .then(|| action.clone())
                    })
                } else {
                    None
                }
            } else if state.dialog.is_some() {
                match mouse.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        if state
                            .editor_area
                            .contains(ratatui::layout::Position::new(mouse.column, mouse.row))
                        {
                            let left = match &state.dialog {
                                Some(RemoteDialogV1::Commit(editor)) => editor.left_column,
                                _ => 0,
                            };
                            Some(RemoteBrowseActionV1::CommitClick(
                                left + usize::from(mouse.column - state.editor_area.x),
                            ))
                        } else {
                            state.hits.iter().rev().find_map(|(rect, action)| {
                                rect.contains(ratatui::layout::Position::new(
                                    mouse.column,
                                    mouse.row,
                                ))
                                .then(|| action.clone())
                            })
                        }
                    }
                    MouseEventKind::ScrollDown => Some(RemoteBrowseActionV1::Move(3)),
                    MouseEventKind::ScrollUp => Some(RemoteBrowseActionV1::Move(-3)),
                    _ => None,
                }
            } else {
                state
                    .workspace
                    .as_ref()
                    .and_then(|ws| ws.mouse(mouse))
                    .or_else(|| {
                        if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                            state.hits.iter().find_map(|(rect, action)| {
                                rect.contains(ratatui::layout::Position::new(
                                    mouse.column,
                                    mouse.row,
                                ))
                                .then(|| action.clone())
                            })
                        } else {
                            None
                        }
                    })
            }
        }
        _ => None,
    };
    if let Some(action) = action {
        dispatch(app, action);
    }
}
pub fn draw(frame: &mut ratatui::Frame, app: &mut App) {
    use ratatui::{
        layout::Rect,
        widgets::{Block, Borders, Clear, Paragraph},
    };
    let theme = app.theme();
    let area = frame.area();
    app.hits.clear();
    let Some(state) = &mut app.remote_repository else {
        return;
    };
    state.hits.clear();
    state.editor_area = Default::default();
    if state.pending.is_none() && state.dialog.is_none() {
        if let Some(ws) = &mut state.workspace {
            ws.draw(frame, area, theme);
            if let Some(actor) = &ws.actor {
                frame.render_widget(
                    crate::identity::AvatarWidget {
                        identity: actor,
                        image: app.avatar_states.ready(actor),
                        theme,
                    },
                    Rect::new(area.right().saturating_sub(6), area.y + 1, 4, 2),
                );
            }
            return;
        }
        frame.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Remote repository • default snapshot unavailable")
            .style(theme.base());
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(Paragraph::new("Default snapshot unavailable • no main/HEAD guessed. Choose an explicit branch, tag or full commit for a fresh bounded read."),inner);
        for (row, buttons) in [
            vec![
                ("[B Branches]", RemoteBrowseActionV1::Branches),
                ("[T Tags]", RemoteBrowseActionV1::Tags),
            ],
            vec![
                ("[C Commit]", RemoteBrowseActionV1::CommitFocus),
                ("[Esc Back]", RemoteBrowseActionV1::Back),
            ],
        ]
        .into_iter()
        .enumerate()
        {
            let mut x = inner.x;
            for (label, action) in buttons {
                let width = (label.len() as u16).min(inner.right().saturating_sub(x));
                let rect = Rect::new(x, inner.bottom().saturating_sub(2) + row as u16, width, 1);
                frame.render_widget(
                    Paragraph::new(label).style(theme.style(crate::theme::SemanticToken::Focus)),
                    rect,
                );
                state.hits.push((rect, action));
                x += width + 1;
            }
        }
        return;
    }
    frame.render_widget(Clear, area);
    let title = match &state.dialog {
        Some(RemoteDialogV1::Commit(_)) => "Remote exact saved commit",
        Some(RemoteDialogV1::Handoff { .. }) => "Matching local snapshot • explicit handoff",
        Some(RemoteDialogV1::Notice { .. }) => "Remote repository details",
        None => "Remote repository • read pending",
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .style(theme.base());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height < 4 {
        return;
    }
    if state.pending.is_some() {
        frame.render_widget(
            Paragraph::new(
                "Fresh bounded read pending • old private tree/content hidden • Esc cancels",
            ),
            inner,
        );
        let cancel = "[Esc Cancel]";
        let rect = Rect::new(
            inner.x,
            inner.bottom() - 1,
            (cancel.len() as u16).min(inner.width),
            1,
        );
        frame.render_widget(
            Paragraph::new(cancel).style(theme.style(crate::theme::SemanticToken::Focus)),
            rect,
        );
        state.hits.push((rect, RemoteBrowseActionV1::Back));
        return;
    }
    let body = Rect::new(
        inner.x,
        inner.y,
        inner.width,
        inner.height.saturating_sub(3),
    );
    let (confirm, action) = match state.dialog.as_mut() {
        Some(RemoteDialogV1::Commit(editor)) => {
            frame.render_widget(
                Paragraph::new(
                    "Full40/64hex saved commit • literal ID only, no branch/HEAD guessing",
                ),
                Rect::new(body.x, body.y, body.width, 1),
            );
            editor.ensure_visible(usize::from(body.width), 1);
            state.editor_area = Rect::new(body.x, body.y + 2, body.width, 1);
            frame.render_widget(
                Paragraph::new(crate::editor::display_line(
                    if editor.text.is_empty() {
                        "[click or type full saved SHA]"
                    } else {
                        &editor.text
                    },
                    editor.left_column,
                    usize::from(body.width),
                ))
                .style(theme.style(crate::theme::SemanticToken::Focus)),
                state.editor_area,
            );
            let cursor = editor
                .position()
                .1
                .saturating_sub(editor.left_column)
                .min(usize::from(body.width.saturating_sub(1)));
            frame.set_cursor_position((body.x + cursor as u16, body.y + 2));
            if let Some(error) = state.input_error {
                frame.render_widget(
                    Paragraph::new(error),
                    Rect::new(body.x, body.y + 4, body.width, 1),
                );
            }
            ("[Enter Browse]", RemoteBrowseActionV1::Commit)
        }
        Some(RemoteDialogV1::Handoff { original, scroll }) => {
            let lines = crate::ui::notice_lines(&original.lines().join("\n"), body.width);
            frame.render_widget(
                Paragraph::new(lines).scroll(((*scroll).min(u16::MAX as usize) as u16, 0)),
                body,
            );
            ("[Enter Confirm]", RemoteBrowseActionV1::Open)
        }
        Some(RemoteDialogV1::Notice { lines, scroll }) => {
            let wrapped = crate::ui::notice_lines(&lines.join("\n"), body.width);
            frame.render_widget(
                Paragraph::new(wrapped).scroll(((*scroll).min(u16::MAX as usize) as u16, 0)),
                body,
            );
            ("[Enter Back]", RemoteBrowseActionV1::Back)
        }
        None => ("[Enter Back]", RemoteBrowseActionV1::Back),
    };
    for (row, buttons) in [
        vec![
            (confirm, action),
            ("[Esc Back]", RemoteBrowseActionV1::Back),
        ],
        if matches!(state.dialog, Some(RemoteDialogV1::Commit(_))) {
            vec![
                ("[Ctrl-U Clear]", RemoteBrowseActionV1::CommitClear),
                ("[Ctrl-A All]", RemoteBrowseActionV1::CommitSelectAll),
            ]
        } else {
            vec![]
        },
        vec![
            ("[PgUp]", RemoteBrowseActionV1::Page(-8)),
            ("[PgDn]", RemoteBrowseActionV1::Page(8)),
        ],
    ]
    .into_iter()
    .enumerate()
    {
        let mut x = inner.x;
        for (index, (label, action)) in buttons.into_iter().enumerate() {
            let width = (label.len() as u16).min(inner.right().saturating_sub(x));
            if width == 0 {
                break;
            }
            let rect = Rect::new(x, inner.bottom() - 3 + row as u16, width, 1);
            frame.render_widget(
                Paragraph::new(label).style(if row == 0 && index == state.dialog_focus {
                    theme.selected(crate::theme::SemanticToken::Focus)
                } else {
                    theme.style(crate::theme::SemanticToken::Focus)
                }),
                rect,
            );
            state.hits.push((rect, action));
            x += width + 1;
        }
    }
}

#[cfg(test)]
#[path = "remote_repository_native_v1_tests.rs"]
mod tests;
/// Outside draw only. Public avatar bytes are display metadata, never access.
/// Hidden local/Catalogue rows are not projected while this surface is owned.
pub fn sync_avatars(
    app: &mut App,
    cache: &mut Option<(String, crate::identity::AvatarCache)>,
    directory: &Path,
    enabled: bool,
) -> bool {
    let Some(state) = &app.remote_repository else {
        return false;
    };
    if !enabled
        || app.theme().mode != crate::theme::ColorMode::TrueColor
        || state.pending.is_some()
        || state.dialog.is_some()
    {
        app.avatar_states.clear();
        return true;
    }
    let Some((identity, actor, context)) = state.workspace.as_ref().and_then(|ws| {
        ws.owner_actor
            .as_ref()
            .map(|actor| (&ws.identity, actor, ws.canonical_name.as_str()))
    }) else {
        app.avatar_states.clear();
        return true;
    };
    let Some(scope) = crate::actor_avatar_states_v1::partition(identity) else {
        app.avatar_states.retire_owned_cache(cache);
        return true;
    };
    if cache
        .as_ref()
        .is_none_or(|(previous, _)| previous != &scope)
    {
        app.avatar_states.retire_owned_cache(cache);
        if let Some(fetcher) = crate::identity::CurlAvatarFetcher::detect(Default::default()) {
            *cache = Some((
                scope.clone(),
                crate::identity::AvatarCache::new(directory.to_owned(), scope, Arc::new(fetcher)),
            ));
        }
    }
    if let Some((_, cache)) = cache {
        let _ = app.avatar_states.sync_from_cache(
            identity,
            context,
            cache,
            std::slice::from_ref(actor),
        );
    } else {
        app.avatar_states.clear();
    }
    true
}
