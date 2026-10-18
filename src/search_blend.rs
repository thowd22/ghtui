//! Explicit blended-search preferences and prepared controls. All storage and
//! transport belongs in workers; input only changes intent and invalidates hits.
pub mod worker;
use crate::{
    app::{Action, App, HitRegion, Overlay},
    search::{
        fusion::{
            blend::BlendPolicy,
            ui::{BlendAction, BlendControls},
        },
        workspace::SearchAction,
        SearchMode,
    },
    storage::{Cache, Scope},
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::Rect,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

const POLICY_KEY: &str = "search-blend-policy:v1";

/// A completed worker's exact preference state, distinct from the immediate
/// input intent. This is preparation only, never confirmation of a search.
pub fn saved_status(policy: &BlendPolicy) -> String {
    let flag = |value| if value { "on" } else { "off" };
    format!("Saved blend preferences • {} • final={} source={} pages={} • Code={} PRs={} Issues={} • no query sent",
        flag(policy.enabled),policy.max_results,policy.per_source_results,policy.per_source_pages,
        flag(policy.sources.contains(&crate::search::fusion::RankingSource::GithubCode)),
        flag(policy.sources.contains(&crate::search::fusion::RankingSource::GithubPullRequests)),
        flag(policy.sources.contains(&crate::search::fusion::RankingSource::GithubIssues)))
}

pub fn load_policy(cache: &Cache, scope: &Scope) -> Result<BlendPolicy, String> {
    if scope.host == "local" {
        return Ok(BlendPolicy::default());
    }
    let Some(saved) = cache
        .saved_view_bounded(scope, POLICY_KEY, 8192)
        .map_err(|error| error.to_string())?
    else {
        return Ok(BlendPolicy::default());
    };
    if saved["version"] != 1 {
        return Err(
            "Saved blend preferences have an unsupported version; no remote query was made".into(),
        );
    }
    let policy: BlendPolicy = serde_json::from_value(saved["policy"].clone()).map_err(|_| {
        "Saved blend preferences are malformed; no remote query was made".to_owned()
    })?;
    policy.validate().map_err(|error| error.to_string())?;
    Ok(policy)
}

#[cfg(test)]
#[path = "search_blend/tests.rs"]
mod tests;
pub fn save_policy(cache: &Cache, scope: &Scope, policy: &BlendPolicy) -> Result<(), String> {
    let id = scope
        .login
        .strip_prefix("user-id:")
        .and_then(|id| id.parse::<u64>().ok())
        .filter(|id| *id > 0)
        .ok_or("Verify a positive GitHub account before saving blend preferences")?;
    if Scope::github(scope.host.to_ascii_lowercase(), Some(id))
        .map_err(|error| error.to_string())?
        != *scope
    {
        return Err("Blend preferences require the exact canonical verified account scope".into());
    }
    policy.validate().map_err(|error| error.to_string())?;
    cache
        .save_view(
            scope,
            POLICY_KEY,
            &serde_json::json!({"version":1,"policy":policy}),
        )
        .map_err(|error| error.to_string())
}
fn mode(app: &App) -> SearchMode {
    app.search
        .as_ref()
        .map_or(SearchMode::Exact, |search| search.mode)
}
fn scope_ready(app: &App) -> bool {
    app.search
        .as_ref()
        .is_some_and(|search| !search.remote_repositories.is_empty())
}
fn controls(app: &App) -> BlendControls {
    BlendControls {
        policy: app
            .search
            .as_ref()
            .map_or_else(BlendPolicy::default, |search| search.blend_policy.clone()),
        focus: app.focus,
    }
}
fn disabled_reason(app: &App) -> String {
    if app.loading[crate::app::View::Search.index()] {
        return "Search is still preparing; cancel or wait before changing blend preferences. No new query was sent.".into();
    }
    if mode(app) != SearchMode::Ranked {
        return "GitHub blending is explicit Ranked-only. Exact and Regex never become remote discovery; choose Ranked to change these preferences.".into();
    }
    if !scope_ready(app) {
        return "Choose explicit GitHub repositories through the launch scope, a saved set or mapped repositories first. A local checkout is not inferred to be a GitHub repository.".into();
    }
    "Keep at least one source enabled while blending, and stay within the displayed result/page limits. This control cannot broaden the requested scope.".into()
}

pub fn open(app: &mut App) {
    app.search_controls.editing = None;
    app.search_controls.corpus = false;
    app.overlay = Some(Overlay::SearchBlend);
    app.focus = 0;
}
pub fn dispatch(app: &mut App, action: BlendAction) {
    if action == BlendAction::Close {
        app.overlay = None;
        app.focus = 0;
        return;
    }
    if app.loading[crate::app::View::Search.index()] {
        app.dispatch(Action::Explain(disabled_reason(app)));
        return;
    }
    let mut prepared = controls(app);
    if !prepared.apply(action, mode(app), scope_ready(app)) {
        app.dispatch(Action::Explain(disabled_reason(app)));
        return;
    }
    if let Err(error) = prepared.policy.validate() {
        app.dispatch(Action::Explain(error.to_string()));
        return;
    }
    let Some(search) = &mut app.search else {
        app.dispatch(Action::Explain(
            "Open Search before changing its blend preferences".into(),
        ));
        return;
    };
    search.blend_policy = prepared.policy;
    search.page = None;
    search.preview = None;
    search.remote_items.clear();
    search.blend_evidence = None;
    app.search_action = Some(SearchAction::BlendPreferences);
    app.search_controls.selected = 0;
    app.search_controls.top = 0;
    app.search_controls.preview_scroll = 0;
    app.status="Blend preferences changed • verifying account before saving; Enter Search explicitly to run a query".into();
}

/// Overlay input is captured even when the pointer is outside a control. Each
/// keyboard/click resolves the same action and the same disabled explanation.
pub fn event(app: &mut App, event: &Event) -> bool {
    if app.overlay != Some(Overlay::SearchBlend) {
        return false;
    }
    let prepared = controls(app);
    let controls = prepared.controls(mode(app), scope_ready(app));
    match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            if key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            {
                return true;
            }
            match key.code {
                KeyCode::Char('E') => app.dispatch(Action::SearchRankDetails),
                KeyCode::Tab | KeyCode::Down => app.focus = (app.focus + 1) % (controls.len() + 1),
                KeyCode::BackTab | KeyCode::Up => {
                    app.focus = (app.focus + controls.len()) % (controls.len() + 1)
                }
                KeyCode::Enter => {
                    if let Some(control) = controls.get(app.focus) {
                        dispatch(app, control.action);
                    } else if app.focus == controls.len() {
                        app.dispatch(Action::SearchRankDetails);
                    }
                }
                code => {
                    if let Some((index, control)) = controls
                        .iter()
                        .enumerate()
                        .find(|(_, control)| control.shortcut == code)
                    {
                        app.focus = index;
                        dispatch(app, control.action);
                    }
                }
            }
        }
        Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
            if let Some(action) = app
                .hits
                .iter()
                .find(|hit| hit.rect.contains((mouse.column, mouse.row).into()))
                .map(|hit| hit.action.clone())
            {
                app.dispatch(action);
            }
        }
        Event::Mouse(mouse)
            if matches!(
                mouse.kind,
                MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
            ) =>
        {
            app.focus = if mouse.kind == MouseEventKind::ScrollDown {
                (app.focus + 1).min(controls.len())
            } else {
                app.focus.saturating_sub(1)
            };
        }
        _ => {}
    }
    true
}
pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let width = area.width.saturating_sub(4).min(78);
    let height = area.height.saturating_sub(2).min(21);
    let panel = Rect::new(
        area.x + (area.width.saturating_sub(width)) / 2,
        area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, panel);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title(" Blend controls • explicit Ranked GitHub discovery ");
    let inner = block.inner(panel);
    frame.render_widget(block, panel);
    app.hits.clear();
    if inner.height == 0 || inner.width == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new("Preferences only • queries run on Enter Search; independent source caps"),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    let prepared = controls(app);
    let controls = prepared.controls(mode(app), scope_ready(app));
    let control_area = Rect::new(
        inner.x,
        inner.y.saturating_add(1),
        inner.width,
        inner.height.saturating_sub(2),
    );
    for hit in
        crate::search::fusion::ui::draw(frame, control_area, &controls, app.focus, app.theme())
    {
        let control = &controls[hit.index];
        app.hits.push(HitRegion {
            rect: hit.rect,
            action: if control.action == BlendAction::Close
                || control.enabled && !app.loading[crate::app::View::Search.index()]
            {
                Action::SearchBlend(control.action)
            } else {
                Action::Explain(disabled_reason(app))
            },
        });
    }
    if inner.height > 1 {
        let footer = Rect::new(inner.x, inner.bottom() - 1, inner.width, 1);
        frame.render_widget(
            Paragraph::new("[E Rank sources] • prepared coverage; no new query").style(
                if app.focus == controls.len() {
                    app.theme().style(crate::theme::SemanticToken::Focus)
                } else {
                    app.theme().base()
                },
            ),
            footer,
        );
        app.hits.push(HitRegion {
            rect: footer,
            action: Action::SearchRankDetails,
        });
    }
}
