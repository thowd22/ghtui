//! TASK-118: moving between worktrees in the Repository view, and the stale `core.worktree`
//! screen.
//!
//! Opening a worktree from the Overview stack reloads the Repository view from that folder in
//! place (read-only: nothing is checked out). The first worktree shown is remembered so the
//! Branch panel can name where you are (`Worktree: <branch>`) and offer the way back.
//!
//! A checkout whose `core.worktree` points at another folder no longer dead-ends on an error:
//! the screen explains the setting in plain words and offers to open the folder Git uses, to
//! list the repository's linked worktrees (read from the Git directory), or, behind
//! `m Local operations`, to unset the setting after a preview and an explicit confirmation.
//! The confirmed write refuses when the setting changed since the preview.
use crate::{
    app::{Action, App, HitRegion, View},
    footer::KeyControl,
    git::CoreWorktreeMismatchV118,
    theme::SemanticToken,
};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Clear, Paragraph},
    Frame,
};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorktreeActionV118 {
    /// Back to the worktree shown before another one was opened.
    Back,
    /// Open the folder a stale `core.worktree` points at.
    OpenConfigured,
    /// Show or hide the linked worktrees on the stale-setting screen.
    ToggleLinked,
    /// Open one listed worktree in the Repository view.
    Open(PathBuf),
    /// Show what unsetting `core.worktree` would do.
    PreviewUnset,
    /// Run the previewed unset.
    ConfirmUnset,
    /// Leave the preview without changing anything.
    CancelUnset,
}

/// The stale-setting screen: what Git reported, and where the keyboard is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MismatchStateV118 {
    pub mismatch: CoreWorktreeMismatchV118,
    pub show_linked: bool,
    pub preview: bool,
    pub selected: usize,
}

fn same(a: &Path, b: &Path) -> bool {
    a == b
        || std::fs::canonicalize(a)
            .ok()
            .zip(std::fs::canonicalize(b).ok())
            .is_some_and(|(a, b)| a == b)
}

/// "Back to ~/repo" while the Repository view shows another worktree than the first one.
pub fn away(app: &App) -> Option<String> {
    let home = app.repository_home_v118.as_ref()?;
    let workspace = app.workspace.as_ref()?;
    (!same(home, &workspace.directory)).then(|| format!("Back to {}", crate::git::tilde_v118(home)))
}

/// "agent/x · ~/trees/x" for the Branch panel title while away from the first worktree.
pub fn here_label(app: &App) -> Option<String> {
    away(app)?;
    let workspace = app.workspace.as_ref()?;
    Some(format!(
        "{} · {}",
        workspace
            .status
            .branch
            .as_deref()
            .map(crate::providers::runner::sanitize_terminal)
            .unwrap_or_else(|| "detached HEAD".into()),
        crate::git::tilde_v118(&workspace.directory)
    ))
}

/// Opens `path` in the Repository view, remembering the first worktree for the way back.
pub fn open(app: &mut App, path: PathBuf) {
    let current = app
        .workspace
        .as_ref()
        .map(|workspace| workspace.directory.clone())
        .or_else(|| {
            app.core_worktree_v118
                .as_ref()
                .map(|state| state.mismatch.checkout.clone())
        });
    if app.repository_home_v118.is_none() {
        app.repository_home_v118 = current;
    }
    if app
        .repository_home_v118
        .as_ref()
        .is_some_and(|home| same(home, &path))
    {
        app.repository_home_v118 = None;
    }
    // Read-only: a fresh Overview load of the chosen worktree; nothing is checked out.
    app.repository_directory_request_v41 = Some(path);
    app.repository_selection = None;
    app.repository_mode = crate::workspace::RepositoryMode::Overview;
    app.repository_load_requested = true;
    app.repository_list.selected = 0;
    app.repository_list.top = 0;
    app.user_navigated = true;
    app.repository_content_focused = true;
    app.status = "Opening worktree • read-only, nothing is checked out".into();
}

/// Installs the stale-setting screen from a Repository read; the previous model is retired.
pub fn install(app: &mut App, mismatch: CoreWorktreeMismatchV118) {
    let keep = app
        .core_worktree_v118
        .as_ref()
        .filter(|state| state.mismatch == mismatch)
        .map(|state| state.show_linked);
    app.workspace = None;
    app.repository_source_identity = None;
    app.core_worktree_v118 = Some(MismatchStateV118 {
        mismatch,
        show_linked: keep.unwrap_or(false),
        preview: false,
        selected: 0,
    });
}

/// The Repository read of `directory`, or the stale `core.worktree` it hit. `opened` is an
/// explicit choice of that folder (a worktree or the configured folder): a folder that is not
/// a checkout is then an error, not an empty view.
pub fn load(
    directory: &Path,
    mode: crate::workspace::RepositoryMode,
    revision: &str,
    previous_tree: Option<crate::repository_view::TreeBrowser>,
    cancel: crate::providers::runner::CancellationToken,
    opened: bool,
) -> Result<
    (
        Option<crate::workspace::Workspace>,
        Option<CoreWorktreeMismatchV118>,
    ),
    String,
> {
    match crate::workspace::Workspace::load_with_cancellation(
        directory,
        mode,
        revision,
        previous_tree,
        cancel.clone(),
    ) {
        Ok(None) if opened => Err(format!(
            "{} is not a Git checkout; nothing was opened",
            crate::git::tilde_v118(directory)
        )),
        Ok(workspace) => Ok((workspace, None)),
        Err(error) => match crate::git::core_worktree_mismatch_v118(directory, &cancel) {
            Some(mismatch) => Ok((None, Some(mismatch))),
            None => Err(error),
        },
    }
}

/// The confirmed unset, then a fresh Overview read of the checkout.
pub fn unset_and_load(
    mismatch: &CoreWorktreeMismatchV118,
    cancel: crate::providers::runner::CancellationToken,
) -> Result<
    (
        Option<crate::workspace::Workspace>,
        Option<CoreWorktreeMismatchV118>,
    ),
    String,
> {
    crate::git::unset_core_worktree_v118(mismatch, &cancel)?;
    load(
        &mismatch.checkout,
        crate::workspace::RepositoryMode::Overview,
        "HEAD",
        None,
        cancel,
        true,
    )
}

/// The screen's choices, in order, with the action each one runs.
fn choices(state: &MismatchStateV118) -> Vec<(String, Option<WorktreeActionV118>)> {
    let mismatch = &state.mismatch;
    if state.preview {
        return vec![
            (
                "y  Confirm: unset core.worktree".into(),
                Some(WorktreeActionV118::ConfirmUnset),
            ),
            (
                "Esc  Keep the setting: change nothing".into(),
                Some(WorktreeActionV118::CancelUnset),
            ),
        ];
    }
    let mut rows = vec![(
        format!(
            "O  Open the folder Git uses: {}{}",
            crate::git::tilde_v118(&mismatch.configured),
            if mismatch.configured_exists {
                ""
            } else {
                " (missing)"
            }
        ),
        mismatch
            .configured_exists
            .then_some(WorktreeActionV118::OpenConfigured),
    )];
    rows.push((
        format!(
            "W  {} the linked worktrees of this repository ({})",
            if state.show_linked { "Hide" } else { "Show" },
            mismatch.linked.len()
        ),
        (!mismatch.linked.is_empty()).then_some(WorktreeActionV118::ToggleLinked),
    ));
    if state.show_linked {
        for (path, branch, missing) in &mismatch.linked {
            rows.push((
                format!(
                    "     ⎇ {} · {}{}",
                    branch
                        .as_deref()
                        .map(crate::providers::runner::sanitize_terminal)
                        .unwrap_or_else(|| "detached HEAD".into()),
                    crate::git::tilde_v118(path),
                    if *missing { " · folder missing" } else { "" }
                ),
                (!missing).then(|| WorktreeActionV118::Open(path.clone())),
            ));
        }
    }
    rows.push((
        "m  Local operations: unset core.worktree (preview first)".into(),
        Some(WorktreeActionV118::PreviewUnset),
    ));
    rows
}

fn explanation(state: &MismatchStateV118) -> Vec<(String, SemanticToken)> {
    let mismatch = &state.mismatch;
    let mut lines = vec![
        (
            "This checkout's Git settings point at another folder".to_owned(),
            SemanticToken::Changed,
        ),
        (
            format!(
                "  Folder you opened:  {}",
                crate::git::tilde_v118(&mismatch.checkout)
            ),
            SemanticToken::Normal,
        ),
        (
            format!(
                "  Git uses instead:   {}",
                crate::git::tilde_v118(&mismatch.configured)
            ),
            SemanticToken::Normal,
        ),
        (
            format!(
                "  Because of:         core.worktree = {}",
                crate::providers::runner::sanitize_terminal(&mismatch.setting)
            ),
            SemanticToken::Muted,
        ),
        (
            format!(
                "                      in {}",
                crate::git::tilde_v118(&mismatch.git_dir.join("config"))
            ),
            SemanticToken::Muted,
        ),
        (
            "So git status, diffs and ghtui read that folder, not this one.".into(),
            SemanticToken::Normal,
        ),
        (String::new(), SemanticToken::Normal),
    ];
    if state.preview {
        lines.extend([
            (
                "Preview: unset core.worktree".to_owned(),
                SemanticToken::Head,
            ),
            // The command and its file on their own lines, so a long path never hides it.
            (
                "  Command:  git config --unset core.worktree".to_owned(),
                SemanticToken::Normal,
            ),
            (
                format!(
                    "  In file:  {}",
                    crate::git::tilde_v118(&mismatch.git_dir.join("config"))
                ),
                SemanticToken::Muted,
            ),
            (
                format!(
                    "  Effect:   Git uses {} as this checkout's working tree again.",
                    crate::git::tilde_v118(&mismatch.checkout)
                ),
                SemanticToken::Normal,
            ),
            (
                "            Only this one setting is removed; no files change.".into(),
                SemanticToken::Normal,
            ),
            (
                "  If another tool still needs it, it can set it again.".into(),
                SemanticToken::Muted,
            ),
            (String::new(), SemanticToken::Normal),
        ]);
    } else {
        lines.push(("What do you want to do?".into(), SemanticToken::Head));
    }
    lines
}

/// The key bar while the stale-setting screen is shown; `None` otherwise.
pub fn footer_keys(app: &App) -> Option<Vec<KeyControl>> {
    let state = app
        .core_worktree_v118
        .as_ref()
        .filter(|_| app.workspace.is_none())?;
    let mut keys = if state.preview {
        vec![
            KeyControl::new(
                "y",
                "Confirm unset",
                Action::WorktreeV118(WorktreeActionV118::ConfirmUnset),
            )
            .with_priority(1),
            KeyControl::new(
                "Esc",
                "Keep setting",
                Action::WorktreeV118(WorktreeActionV118::CancelUnset),
            )
            .with_priority(2),
        ]
    } else {
        let mismatch = &state.mismatch;
        vec![
            KeyControl::new(
                "O",
                "Open folder Git uses",
                Action::WorktreeV118(WorktreeActionV118::OpenConfigured),
            )
            .with_compact("Open")
            .with_priority(1)
            .disabled_by(
                (!mismatch.configured_exists)
                    .then(|| "The folder core.worktree points at does not exist".into()),
            ),
            KeyControl::new(
                "W",
                "Linked worktrees",
                Action::WorktreeV118(WorktreeActionV118::ToggleLinked),
            )
            .with_compact("Worktrees")
            .with_priority(2)
            .disabled_by(
                mismatch
                    .linked
                    .is_empty()
                    .then(|| "This repository has no linked worktrees".into()),
            ),
            KeyControl::new(
                "m",
                "Local operations",
                Action::WorktreeV118(WorktreeActionV118::PreviewUnset),
            )
            .with_compact("Git ops")
            .with_priority(3),
        ]
    };
    keys.extend(crate::footer::transitional_keys(app));
    Some(keys)
}

/// Arrows move between the screen's choices and Enter runs the selected one.
pub fn key(app: &mut App, key: KeyEvent) -> bool {
    if app.workspace.is_some() || app.view != View::Repository {
        return false;
    }
    let Some(state) = app.core_worktree_v118.as_mut() else {
        return false;
    };
    let rows = choices(state);
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            state.selected = state.selected.saturating_sub(1);
            true
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.selected = (state.selected + 1).min(rows.len().saturating_sub(1));
            true
        }
        KeyCode::Enter => {
            let action = rows
                .get(state.selected)
                .and_then(|(_, action)| action.clone());
            match action {
                Some(action) => dispatch(app, action),
                None => app.status = "That choice is unavailable".into(),
            }
            true
        }
        _ => false,
    }
}

pub fn dispatch(app: &mut App, action: WorktreeActionV118) {
    match action {
        WorktreeActionV118::Back => match app.repository_home_v118.clone() {
            Some(home) => open(app, home),
            None => app.status = "Already on the first worktree".into(),
        },
        WorktreeActionV118::Open(path) => open(app, path),
        WorktreeActionV118::OpenConfigured => {
            let Some(path) = app
                .core_worktree_v118
                .as_ref()
                .filter(|state| state.mismatch.configured_exists)
                .map(|state| state.mismatch.configured.clone())
            else {
                app.status = "The folder core.worktree points at does not exist".into();
                return;
            };
            open(app, path);
        }
        WorktreeActionV118::ToggleLinked => {
            if let Some(state) = &mut app.core_worktree_v118 {
                if state.mismatch.linked.is_empty() {
                    app.status = "This repository has no linked worktrees".into();
                    return;
                }
                state.show_linked = !state.show_linked;
                state.selected = 1;
            }
        }
        WorktreeActionV118::PreviewUnset => {
            if let Some(state) = &mut app.core_worktree_v118 {
                state.preview = true;
                // Cancel is the safe default; confirming takes `y` or a deliberate choice.
                state.selected = 1;
                app.status = "Preview only • nothing changed yet • y confirms, Esc cancels".into();
            }
        }
        WorktreeActionV118::CancelUnset => {
            if let Some(state) = &mut app.core_worktree_v118 {
                state.preview = false;
                state.selected = 0;
                app.status = "Cancelled • core.worktree was not changed".into();
            }
        }
        WorktreeActionV118::ConfirmUnset => {
            let Some(state) = app
                .core_worktree_v118
                .as_ref()
                .filter(|state| state.preview)
            else {
                app.status = "Preview the change first: m Local operations".into();
                return;
            };
            app.core_worktree_unset_v118 = Some(state.mismatch.clone());
            app.loading[View::Repository.index()] = true;
            app.status = "Unsetting core.worktree…".into();
        }
    }
}

/// Draws the stale-setting screen into the Repository pane. Returns whether it was drawn.
pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) -> bool {
    let Some(state) = app
        .core_worktree_v118
        .as_ref()
        .filter(|_| app.workspace.is_none())
    else {
        return false;
    };
    let theme = app.theme();
    frame.render_widget(Clear, area);
    frame.render_widget(ratatui::widgets::Block::default().style(theme.base()), area);
    let mut y = area.y;
    for (text, token) in explanation(state) {
        if y >= area.bottom() {
            return true;
        }
        frame.render_widget(
            Paragraph::new(crate::app::safe_text(&text)).style(theme.style(token)),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;
    }
    let selected = state.selected;
    for (index, (text, action)) in choices(state).into_iter().enumerate() {
        if y >= area.bottom() {
            break;
        }
        let rect = Rect::new(area.x, y, area.width, 1);
        y += 1;
        let token = if action.is_some() {
            SemanticToken::Reference
        } else {
            SemanticToken::Muted
        };
        let style = if index == selected {
            theme.selected(token)
        } else {
            theme.style(token)
        };
        let marker = if index == selected { "▌ " } else { "  " };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(marker, style),
                Span::styled(crate::app::safe_text(&text), style),
            ])),
            rect,
        );
        if let Some(action) = action {
            app.hits.push(HitRegion {
                rect,
                action: Action::WorktreeV118(action),
            });
        }
    }
    true
}

#[cfg(test)]
#[path = "repository_worktrees_v118_tests.rs"]
mod tests;
