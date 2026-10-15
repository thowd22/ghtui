//! Original-profile owned association modal. All I/O is worker-only; local
//! Workspace stays untouched so chosen-file/line/responsible-commit return is
//! the actual original navigation, not a reconstructed approximation.
use crate::{
    app::{Action, App, HitRegion, View},
    github_workspace::{GithubAction, GithubWorkspace, PrTab},
    providers::{
        gh::{ClosedRelatedPrsV1, GithubProvider, RelatedPrReadTargetV1},
        local_git_navigation_read_v1::LocalGitNavigationReadV1,
        runner::CancellationToken,
    },
    repository_commit_actor_native_v1::LocalActorRepositoryChoiceV1,
    repository_commit_actor_profile_v1::LocalActorProfileV1,
    theme::SemanticToken,
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::{Position, Rect},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use std::{path::Path, sync::Arc, time::Instant};

#[path = "repository_related_pr_avatars_v1.rs"]
pub mod avatars_v1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelatedPrActionV1 {
    Discover,
    Open(usize),
    Back,
    Cancel,
    Select(i32),
    Scroll(i32),
    Focus(i32),
    Tab(PrTab),
    Explain,
}
pub struct RelatedPrOwnerV1 {
    original_navigation: Arc<crate::navigation_local_source_v1::OriginalSavedVisitV1>,
    profile: Arc<LocalActorProfileV1>,
    choice: Arc<LocalActorRepositoryChoiceV1>,
    operation: Arc<LocalGitNavigationReadV1>,
    opening: Option<(Arc<ClosedRelatedPrsV1>, usize)>,
}
impl RelatedPrOwnerV1 {
    pub fn cancellation(&self) -> &CancellationToken {
        self.operation.cancellation()
    }
    pub fn host(&self) -> &str {
        &self.choice.original_source().source().identity().host
    }
    pub fn guard(&self) -> Result<(), String> {
        self.operation.check()
    }
    pub(crate) fn expired(&self) -> bool {
        Instant::now() >= self.operation.deadline()
    }
    pub fn owns(&self, app: &App) -> bool {
        app.view == View::Repository
            && app.remote_repository.is_none()
            && !app.repository_load_requested
            && app.repository_action.is_none()
            && app.original_tree_work_v1.is_none()
            && app.original_diff_work_v1.is_none()
            && !app.original_diff_cancel_v1
            && app.original_diff_picker_v1.is_none()
            && app.graph_reread_request_v3.is_none()
            && app.navigation_restore.is_none()
            && !app.navigation_restore_inflight
            && !app.cancel_requested
            && !app.local_actor_cancel_requested
            && !app.original_tree_cancel_v1
            && !app.related_pr_cancel_v1
            && !app.refresh_requested
            && !app.local_actor_requested
            && !app.related_pr_requested_v1
            && !self.cancellation().is_cancelled()
            && app.related_pr_v1.as_ref().is_some_and(|state| {
                state
                    .pending
                    .as_ref()
                    .is_some_and(|owner| std::ptr::eq(owner.as_ref(), self))
            })
            && app
                .local_commit_actors
                .profile()
                .is_some_and(|p| Arc::ptr_eq(p, &self.profile))
            && app
                .local_commit_actors
                .choice()
                .is_some_and(|c| Arc::ptr_eq(c, &self.choice))
            && app.workspace.as_ref().is_some_and(|w| {
                crate::navigation_local_source_v1::OriginalSavedVisitV1::capture(w)
                    .is_ok_and(|actual| actual.key() == self.original_navigation.key())
                    && w.actor_origin
                        .as_ref()
                        .is_some_and(|o| Arc::ptr_eq(o, self.profile.origin()))
                    && w.selected_commit
                        .as_ref()
                        .is_some_and(|c| c.id == self.profile.sha())
                    && w.actor_profile == Some(self.profile.role())
            })
    }
}
pub struct PreparedRelatedPrV1 {
    owner: Arc<RelatedPrOwnerV1>,
    closed: ClosedRelatedPrsV1,
}
impl PreparedRelatedPrV1 {
    pub(crate) fn matches(&self, owner: &Arc<RelatedPrOwnerV1>) -> bool {
        !owner.expired() && self.matches_owner(owner)
    }
    pub(crate) fn matches_owner(&self, owner: &Arc<RelatedPrOwnerV1>) -> bool {
        Arc::ptr_eq(&self.owner, owner)
            && self.closed.matches_operation(&owner.operation)
            && Arc::ptr_eq(
                self.closed.original_source(),
                owner.choice.original_source(),
            )
            && self.closed.selected_index() == owner.choice.original_index()
            && self.closed.sha() == owner.profile.sha()
    }
}
pub fn read(
    owner: &Arc<RelatedPrOwnerV1>,
    provider: &GithubProvider,
    database: &Path,
) -> Result<PreparedRelatedPrV1, String> {
    owner.guard()?;
    let raw = Arc::new(
        owner
            .profile
            .origin()
            .read_original_under_navigation_operation_v1(
                owner.profile.sha(),
                &Default::default(),
                &owner.operation,
            )
            .map_err(|e| e.to_string())?,
    );
    let closed = provider
        .related_pr_read_v1(
            RelatedPrReadTargetV1 {
                original: owner.choice.original_source(),
                selected_index: owner.choice.original_index(),
                local: &raw,
                operation: &owner.operation,
            },
            database,
            &Default::default(),
            owner
                .opening
                .as_ref()
                .map(|(source, index)| (source.as_ref(), *index)),
            || {
                owner
                    .guard()
                    .map_err(|message| crate::providers::gh::GithubError {
                        kind: crate::providers::gh::GithubErrorKind::Transport,
                        message,
                    })
            },
        )
        .map_err(|e| e.to_string())?;
    Ok(PreparedRelatedPrV1 {
        owner: owner.clone(),
        closed,
    })
}
pub struct RelatedPrStateV1 {
    original_navigation: Arc<crate::navigation_local_source_v1::OriginalSavedVisitV1>,
    profile: Arc<LocalActorProfileV1>,
    choice: Arc<LocalActorRepositoryChoiceV1>,
    pending: Option<Arc<RelatedPrOwnerV1>>,
    snapshot: Option<Arc<ClosedRelatedPrsV1>>,
    viewer: Option<GithubWorkspace>,
    selected: usize,
    scroll: usize,
    focus: usize,
    hits: Vec<(Rect, RelatedPrActionV1)>,
    avatar_view_v1: Option<avatars_v1::ViewportV1>,
    next: Option<usize>,
    boundary_failed: bool,
    status: String,
}
impl Drop for RelatedPrStateV1 {
    fn drop(&mut self) {
        if let Some(owner) = &self.pending {
            owner.cancellation().cancel();
        }
    }
}
pub fn begin(
    app: &mut App,
    cancellation: CancellationToken,
) -> Result<Arc<RelatedPrOwnerV1>, String> {
    let state = app
        .related_pr_v1
        .as_mut()
        .ok_or("Original related-PR modal is not open")?;
    if state.boundary_failed {
        return Err("Original remote boundary unavailable; return and choose explicitly".into());
    }
    if let Some(old) = state.pending.take() {
        old.cancellation().cancel();
    }
    let opening = state
        .next
        .take()
        .map(|index| {
            state
                .snapshot
                .as_ref()
                .map(|s| (s.clone(), index))
                .ok_or("Original related candidate is unavailable")
        })
        .transpose()?;
    let operation = Arc::new(LocalGitNavigationReadV1::begin(
        state.profile.origin(),
        cancellation,
    )?);
    let owner = Arc::new(RelatedPrOwnerV1 {
        original_navigation: state.original_navigation.clone(),
        profile: state.profile.clone(),
        choice: state.choice.clone(),
        operation,
        opening,
    });
    state.snapshot = None;
    state.viewer = None;
    state.avatar_view_v1 = None;
    state.hits.clear();
    state.status = "Checking exact original commit • previous private bodies hidden".into();
    state.pending = Some(owner.clone());
    Ok(owner)
}
pub fn accept(
    app: &mut App,
    owner: &Arc<RelatedPrOwnerV1>,
    mut result: PreparedRelatedPrV1,
) -> bool {
    if !owner.owns(app) || !result.matches(owner) {
        return false;
    }
    let state = app.related_pr_v1.as_mut().expect("owned modal");
    state.pending = None;
    state.viewer = result.closed.take_page().map(|page| {
        let mut github = GithubWorkspace::empty(
            result.closed.identity().clone(),
            Vec::new(),
            Default::default(),
        );
        github.pr = Some(page);
        github
    });
    state.status = format!(
        "{} observed PRs • {} pages • {} • no complete-history/absence claim",
        result.closed.candidates().len(),
        result.closed.pages(),
        if result.closed.exhausted_subset() {
            "API subset exhausted"
        } else {
            "partial bounded subset"
        }
    );
    state.snapshot = Some(Arc::new(result.closed));
    state.avatar_view_v1 = None;
    state.selected = 0;
    state.scroll = 0;
    state.focus = 0;
    state.hits.clear();
    app.hits.clear();
    app.avatar_states.clear();
    true
}
pub fn fail(app: &mut App, owner: &Arc<RelatedPrOwnerV1>) -> bool {
    if !owner.owns(app) {
        return false;
    }
    let state = app.related_pr_v1.as_mut().expect("owned modal");
    state.pending = None;
    state.snapshot = None;
    state.viewer = None;
    state.avatar_view_v1 = None;
    state.hits.clear();
    state.boundary_failed = true;
    state.status = "Original related PR read unavailable • private repositories/bodies hidden • return and choose explicitly; local Git unchanged".into();
    app.local_commit_actors.clear_host_report();
    app.avatar_states.clear();
    app.hits.clear();
    true
}
pub fn dispatch(app: &mut App, action: RelatedPrActionV1) {
    if action == RelatedPrActionV1::Discover && app.related_pr_v1.is_none() {
        if app.view != View::Repository || app.loading[View::Repository.index()] {
            app.status =
                "Wait for the actual saved-commit profile before related PR discovery".into();
            return;
        }
        let Some(profile) = app.local_commit_actors.profile().cloned() else {
            app.status = "Open the original Author or Committer profile first".into();
            return;
        };
        let Some(choice) = app
            .local_commit_actors
            .choice()
            .cloned()
            .filter(|c| c.matches_profile(&profile))
        else {
            app.status = "Choose repository explicitly before Related PRs • no repository mapping is inferred".into();
            return;
        };
        let original_navigation = match app
            .workspace
            .as_ref()
            .ok_or_else(|| "Original local navigation missing".to_owned())
            .and_then(crate::navigation_local_source_v1::OriginalSavedVisitV1::capture)
        {
            Ok(visit) => visit,
            Err(reason) => {
                app.status = reason;
                return;
            }
        };
        app.local_commit_actors.clear_host_report();
        app.related_pr_v1 = Some(RelatedPrStateV1 {
            original_navigation,
            profile,
            choice,
            pending: None,
            snapshot: None,
            viewer: None,
            selected: 0,
            scroll: 0,
            focus: 0,
            hits: Vec::new(),
            avatar_view_v1: None,
            next: None,
            boundary_failed: false,
            status: String::new(),
        });
        app.related_pr_requested_v1 = true;
        app.hits.clear();
        app.avatar_states.clear();
        return;
    }
    let Some(state) = app.related_pr_v1.as_mut() else {
        return;
    };
    state.avatar_view_v1 = None;
    if state.boundary_failed
        && !matches!(
            action,
            RelatedPrActionV1::Back | RelatedPrActionV1::Cancel | RelatedPrActionV1::Focus(_)
        )
    {
        return;
    }
    match action {
        RelatedPrActionV1::Back if state.viewer.is_some() => {
            state.viewer = None;
            state.scroll = 0;
            state.focus = 0;
        }
        RelatedPrActionV1::Back | RelatedPrActionV1::Cancel => {
            app.related_pr_cancel_v1 = state.pending.is_some();
            app.related_pr_requested_v1 = false;
            app.related_pr_v1 = None;
            app.status =
                "Returned to exact original local navigation • working files unchanged".into();
        }
        RelatedPrActionV1::Discover if state.pending.is_none() => {
            state.next = None;
            state.viewer = None;
            app.related_pr_requested_v1 = true;
        }
        RelatedPrActionV1::Open(index) if state.pending.is_none() && state.viewer.is_none() => {
            if state
                .snapshot
                .as_ref()
                .is_some_and(|s| index < s.candidates().len())
            {
                state.next = Some(index);
                app.related_pr_requested_v1 = true;
            }
        }
        RelatedPrActionV1::Select(delta) => {
            let max = state
                .snapshot
                .as_ref()
                .map_or(0, |s| s.candidates().len().saturating_sub(1));
            state.selected = state
                .selected
                .saturating_add_signed(delta as isize)
                .min(max);
            state.scroll = state.selected;
        }
        RelatedPrActionV1::Scroll(delta) => {
            state.scroll = state.scroll.saturating_add_signed(delta as isize);
        }
        RelatedPrActionV1::Focus(delta) => {
            let count = state.hits.len().max(1);
            state.focus = (state.focus as i64 + i64::from(delta)).rem_euclid(count as i64) as usize;
        }
        RelatedPrActionV1::Tab(tab) if state.viewer.is_some() => {
            if matches!(
                tab,
                PrTab::Summary
                    | PrTab::Conversation
                    | PrTab::Commits
                    | PrTab::Timeline
                    | PrTab::Files
            ) {
                let page = state
                    .viewer
                    .as_mut()
                    .and_then(|v| v.pr.as_mut())
                    .expect("visible PR");
                let old_tab = page.tab;
                page.tab = tab;
                let mut display_limited = false;
                if let Err(error) = page.prepare_rows() {
                    if crate::github_workspace::timeline_projection_v1::is_display_limit(&error) {
                        page.tab = old_tab;
                        display_limited = true;
                        state.status = "Timeline display limit reached • original observed section/selection retained • no extra reads".into();
                    } else {
                        state.viewer = None;
                        state.snapshot = None;
                    }
                }
                if !display_limited {
                    state.scroll = 0;
                }
            } else {
                state.status = "This original association grants no Checks/metadata/mutation authority • use an explicitly activated remote scope".into();
            }
        }
        RelatedPrActionV1::Explain => {
            state.status = "Observed read-only related PR • fresh scope required for additional reads or writes; no mutation submitted".into();
        }
        _ => {}
    }
    if let Some(state) = app.related_pr_v1.as_mut() {
        state.hits.clear();
    }
    app.hits.clear();
}
pub fn event(app: &mut App, event: Event) -> bool {
    retire_invalid(app);
    let Some(state) = app.related_pr_v1.as_ref() else {
        return false;
    };
    let action = match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                app.dispatch(Action::Quit);
                return true;
            }
            match key.code {
                KeyCode::Esc | KeyCode::Char('b') => Some(RelatedPrActionV1::Back),
                KeyCode::Char('r') => Some(RelatedPrActionV1::Discover),
                KeyCode::Tab => Some(RelatedPrActionV1::Focus(1)),
                KeyCode::BackTab => Some(RelatedPrActionV1::Focus(-1)),
                KeyCode::Enter => state.hits.get(state.focus).map(|(_, a)| a.clone()),
                KeyCode::Char('O') => Some(RelatedPrActionV1::Open(state.selected)),
                KeyCode::Up => Some(if state.viewer.is_some() {
                    RelatedPrActionV1::Scroll(-1)
                } else {
                    RelatedPrActionV1::Select(-1)
                }),
                KeyCode::Char('k') if state.viewer.is_none() => Some(RelatedPrActionV1::Select(-1)),
                KeyCode::Down | KeyCode::Char('j') => Some(if state.viewer.is_some() {
                    RelatedPrActionV1::Scroll(1)
                } else {
                    RelatedPrActionV1::Select(1)
                }),
                KeyCode::PageUp => Some(RelatedPrActionV1::Scroll(-12)),
                KeyCode::PageDown => Some(RelatedPrActionV1::Scroll(12)),
                KeyCode::Char(ch) if state.viewer.is_some() => PrTab::ALL
                    .iter()
                    .find(|(_, _, key)| *key == ch)
                    .map(|(tab, _, _)| RelatedPrActionV1::Tab(*tab)),
                _ => None,
            }
        }
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => state
                .hits
                .iter()
                .find(|(rect, _)| rect.contains(Position::new(mouse.column, mouse.row)))
                .map(|(_, a)| a.clone()),
            MouseEventKind::ScrollDown => Some(RelatedPrActionV1::Scroll(3)),
            MouseEventKind::ScrollUp => Some(RelatedPrActionV1::Scroll(-3)),
            _ => None,
        },
        Event::Resize(_, _) => {
            if let Some(state) = app.related_pr_v1.as_mut() {
                state.hits.clear();
                state.avatar_view_v1 = None;
            }
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
pub fn draw(frame: &mut Frame, app: &mut App) -> bool {
    retire_invalid(app);
    let Some(mut state) = app.related_pr_v1.take() else {
        return false;
    };
    app.hits.clear();
    state.hits.clear();
    state.avatar_view_v1 = None;
    let area = frame.area();
    let theme = app.theme();
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Related PRs • exact original saved commit • read-only")
        .style(theme.base());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if !inner.is_empty() {
        frame.render_widget(
            Paragraph::new(format!(
                "{} • {} • {}",
                if state.boundary_failed {
                    "Remote repository unavailable"
                } else {
                    state.choice.label()
                },
                state.profile.sha(),
                crate::repository_commit_actor_native_v1::role_label(state.profile.role())
            ))
            .style(theme.style(SemanticToken::Reference)),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
        let footer = inner.bottom().saturating_sub(1);
        let pending = state.pending.is_some() || app.related_pr_requested_v1;
        let mut controls = if pending {
            vec![("[Esc Cancel]", RelatedPrActionV1::Cancel)]
        } else if state.boundary_failed {
            vec![("[Esc Back]", RelatedPrActionV1::Back)]
        } else {
            vec![
                ("[Esc Back]", RelatedPrActionV1::Back),
                ("[r Discover]", RelatedPrActionV1::Discover),
            ]
        };
        if !pending
            && state.viewer.is_none()
            && state
                .snapshot
                .as_ref()
                .is_some_and(|s| !s.candidates().is_empty())
        {
            controls.push((
                "[O Open selected PR]",
                RelatedPrActionV1::Open(state.selected),
            ));
        }
        let mut x = inner.x;
        for (label, action) in controls {
            let width = crate::editor::display_width(label)
                .min(inner.right().saturating_sub(x) as usize) as u16;
            if width == 0 {
                break;
            }
            let rect = Rect::new(x, footer, width, 1);
            frame.render_widget(
                Paragraph::new(label).style(theme.style(if state.focus == state.hits.len() {
                    SemanticToken::Focus
                } else {
                    SemanticToken::Reference
                })),
                rect,
            );
            state.hits.push((rect, action));
            x += width + 1;
        }
        let status_y = footer.saturating_sub(1);
        frame.render_widget(
            Paragraph::new(state.status.as_str()).style(theme.style(SemanticToken::Muted)),
            Rect::new(inner.x, status_y, inner.width, 1),
        );
        let body = Rect::new(
            inner.x,
            inner.y.saturating_add(1),
            inner.width,
            status_y.saturating_sub(inner.y.saturating_add(1)),
        );
        if !pending {
            if let Some(viewer) = state.viewer.take() {
                // One genuine existing PR renderer, O(1) moves only. Restore
                // all original global view/input state; its actions are scoped
                // below and cannot reach the general mutation dispatcher.
                let previous = app.github.replace(viewer);
                let previous_scroll = std::mem::replace(&mut app.scroll, state.scroll);
                let mapped_focus = state
                    .focus
                    .checked_sub(state.hits.len())
                    .unwrap_or(usize::MAX);
                let previous_focus = std::mem::replace(&mut app.focus, mapped_focus);
                let previous_content = std::mem::replace(&mut app.github_content_focused, false);
                let previous_visible_rows = app.github_visible_rows;
                app.github_visible_rows = 0;
                crate::github_ui::draw(frame, app, body);
                state.avatar_view_v1 = app.github.as_ref().and_then(|viewer| {
                    avatars_v1::ViewportV1::viewer(
                        &state,
                        viewer,
                        if body.width > 2 {
                            app.github_visible_rows
                        } else {
                            0
                        },
                    )
                });
                for hit in app.hits.drain(..) {
                    let action = match hit.action {
                        Action::Github(GithubAction::Back) => RelatedPrActionV1::Back,
                        Action::Github(GithubAction::PrTab(tab)) => RelatedPrActionV1::Tab(tab),
                        _ => RelatedPrActionV1::Explain,
                    };
                    state.hits.push((hit.rect, action));
                }
                state.viewer = std::mem::replace(&mut app.github, previous);
                app.scroll = previous_scroll;
                app.focus = previous_focus;
                app.github_content_focused = previous_content;
                app.github_visible_rows = previous_visible_rows;
            } else if let Some(snapshot) = &state.snapshot {
                state.avatar_view_v1 = avatars_v1::ViewportV1::candidates(
                    &state,
                    if body.width > 2 {
                        body.height as usize
                    } else {
                        0
                    },
                );
                if snapshot.candidates().is_empty() {
                    frame.render_widget(
                        Paragraph::new(
                            "No PRs observed in this API subset • historical absence unknown",
                        ),
                        body,
                    );
                }
                for (i, candidate) in snapshot
                    .candidates()
                    .iter()
                    .enumerate()
                    .skip(state.scroll)
                    .take(body.height as usize)
                {
                    let rect = Rect::new(body.x, body.y + (i - state.scroll) as u16, body.width, 1);
                    let style = theme.style(if i == state.selected {
                        SemanticToken::Focus
                    } else {
                        SemanticToken::Normal
                    });
                    frame.render_widget(
                        Paragraph::new(if i == state.selected { "›" } else { " " }).style(style),
                        Rect::new(rect.x, rect.y, rect.width.min(1), 1),
                    );
                    let actor = candidate.display_actor();
                    let image = match app.avatar_states.get(actor) {
                        Some(crate::identity::AvatarState::Ready(image)) => Some(image.as_ref()),
                        _ => None,
                    };
                    frame.render_widget(
                        crate::identity::AvatarWidget {
                            identity: actor,
                            image,
                            theme,
                        },
                        Rect::new(
                            rect.x + rect.width.min(2),
                            rect.y,
                            rect.width.saturating_sub(2).min(4),
                            1,
                        ),
                    );
                    frame.render_widget(
                        Paragraph::new(candidate.display_label()).style(style),
                        Rect::new(
                            rect.x + rect.width.min(7),
                            rect.y,
                            rect.width.saturating_sub(7),
                            1,
                        ),
                    );
                    state.hits.push((rect, RelatedPrActionV1::Open(i)));
                }
            }
        }
    }
    app.hits
        .extend(state.hits.iter().map(|(rect, action)| HitRegion {
            rect: *rect,
            action: Action::RelatedPrV1(action.clone()),
        }));
    app.related_pr_v1 = Some(state);
    true
}
fn retire_invalid(app: &mut App) {
    let invalid = app.related_pr_v1.as_ref().is_some_and(|state| {
        app.view != View::Repository
            || app.remote_repository.is_some()
            // Failed state carries no remote body/action/authority. It may
            // remain only above the unchanged actual local route after the
            // private controller and catalogue are retired by the boundary.
            || (!state.boundary_failed
                && (app
                    .local_commit_actors
                    .profile()
                    .is_none_or(|p| !Arc::ptr_eq(p, &state.profile))
                    || app
                        .local_commit_actors
                        .choice()
                        .is_none_or(|c| !Arc::ptr_eq(c, &state.choice))))
            || app.workspace.as_ref().is_none_or(|w| {
                !crate::navigation_local_source_v1::OriginalSavedVisitV1::capture(w)
                    .is_ok_and(|actual| actual.key() == state.original_navigation.key())
                    || w.actor_origin
                        .as_ref()
                        .is_none_or(|o| !Arc::ptr_eq(o, state.profile.origin()))
                    || w.selected_commit
                        .as_ref()
                        .is_none_or(|c| c.id != state.profile.sha())
                    || w.actor_profile != Some(state.profile.role())
            })
    });
    if invalid {
        app.related_pr_v1 = None;
        app.related_pr_requested_v1 = false;
        app.related_pr_cancel_v1 = true;
        app.avatar_states.clear();
        app.hits.clear();
    }
}

#[cfg(test)]
#[path = "repository_related_pr_v1_tests.rs"]
pub(crate) mod tests;
#[cfg(test)]
#[path = "repository_related_pr_timeline_v1_tests.rs"]
mod timeline_tests_v1;
