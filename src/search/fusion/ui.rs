//! Provider-free component. The application owns persistence, worker dispatch and
//! scope/journal proofs; drawing and actions here never invoke Git or gh.
use super::{blend::BlendPolicy, RankingSource};
use crate::{
    footer::KeyControl,
    search::SearchMode,
    theme::{SemanticToken, Theme},
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{layout::Rect, style::Modifier, widgets::Paragraph, Frame};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitField {
    FinalResults,
    SourceResults,
    SourcePages,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendAction {
    Toggle,
    Source(RankingSource),
    Adjust(LimitField, i8),
    Close,
}
/// One blend preference control: the same [`KeyControl`] model as the main key bar, so its
/// painted key, click target and keyboard shortcut cannot drift apart. The overlay paints it
/// as a bracketed `[key label]` row.
pub type Control = KeyControl<BlendAction>;

const RANKED_ONLY: &str = "GitHub blending is explicit Ranked-only. Exact and Regex never become remote discovery; choose Ranked to change these preferences.";
const SCOPE_FIRST: &str = "Choose explicit GitHub repositories through the launch scope, a saved set or mapped repositories first. A local checkout is not inferred to be a GitHub repository.";
const LAST_SOURCE: &str = "Keep at least one source enabled while blending. This control cannot broaden the requested scope.";
const LIMIT: &str = "Stay within the displayed result/page limits. This control cannot broaden the requested scope.";

fn unless(enabled: bool, reason: &str) -> Option<String> {
    (!enabled).then(|| reason.to_owned())
}
#[derive(Debug, Clone)]
pub struct ControlHit {
    pub rect: Rect,
    pub index: usize,
}
#[derive(Debug, Clone, Default)]
pub struct BlendControls {
    pub policy: BlendPolicy,
    pub focus: usize,
}
impl BlendControls {
    pub fn controls(&self, mode: SearchMode, remote_scope_ready: bool) -> Vec<Control> {
        let ranked = mode == SearchMode::Ranked;
        let can_toggle = self.policy.enabled || remote_scope_ready;
        let mut controls = vec![KeyControl::new(
            "b",
            format!(
                "Blend GitHub: {}",
                if self.policy.enabled { "on" } else { "off" }
            ),
            BlendAction::Toggle,
        )
        .with_selected(self.policy.enabled)
        .disabled_by(unless(ranked, RANKED_ONLY))
        .disabled_by(unless(can_toggle, SCOPE_FIRST))];
        for (key, source, label) in [
            ("c", RankingSource::GithubCode, "Code"),
            ("p", RankingSource::GithubPullRequests, "PRs"),
            ("i", RankingSource::GithubIssues, "Issues"),
        ] {
            let selected = self.policy.sources.contains(&source);
            controls.push(
                KeyControl::new(
                    key,
                    format!("{label}: {}", if selected { "on" } else { "off" }),
                    BlendAction::Source(source),
                )
                .with_selected(selected)
                .disabled_by(unless(ranked, RANKED_ONLY))
                .disabled_by(unless(
                    !(self.policy.enabled && selected && self.policy.sources.len() == 1),
                    LAST_SOURCE,
                )),
            );
        }
        for (field, value, maximum, decrease, increase, label) in [
            (
                LimitField::FinalResults,
                self.policy.max_results,
                5000,
                "f",
                "g",
                "Final results",
            ),
            (
                LimitField::SourceResults,
                self.policy.per_source_results,
                1000,
                "-",
                "+",
                "Results/source",
            ),
            (
                LimitField::SourcePages,
                self.policy.per_source_pages,
                10,
                "[",
                "]",
                "Pages/source",
            ),
        ] {
            for (delta, key, verb, enabled) in [
                (-1, decrease, "less", value > 1),
                (1, increase, "more", value < maximum),
            ] {
                controls.push(
                    KeyControl::new(
                        key,
                        format!("{label}: {value} {verb}"),
                        BlendAction::Adjust(field, delta),
                    )
                    .disabled_by(unless(ranked, RANKED_ONLY))
                    .disabled_by(unless(enabled, LIMIT)),
                );
            }
        }
        controls.push(KeyControl::new("Esc", "Close", BlendAction::Close));
        controls
    }
    /// The same action descriptor validates mouse, shortcut and Enter dispatch.
    /// Returning true changes preferences only; callers cancel/requery separately.
    pub fn apply(
        &mut self,
        action: BlendAction,
        mode: SearchMode,
        remote_scope_ready: bool,
    ) -> bool {
        if !self
            .controls(mode, remote_scope_ready)
            .iter()
            .any(|control| control.action == action && control.disabled.is_none())
        {
            return false;
        }
        match action {
            BlendAction::Toggle => self.policy.enabled = !self.policy.enabled,
            BlendAction::Source(source) => {
                if !self.policy.sources.remove(&source) {
                    self.policy.sources.insert(source);
                }
            }
            BlendAction::Adjust(field, delta) => {
                let (value, maximum) = match field {
                    LimitField::FinalResults => (&mut self.policy.max_results, 5000),
                    LimitField::SourceResults => (&mut self.policy.per_source_results, 1000),
                    LimitField::SourcePages => (&mut self.policy.per_source_pages, 10),
                };
                *value = value
                    .saturating_add_signed(delta as isize)
                    .clamp(1, maximum);
            }
            BlendAction::Close => return false,
        }
        true
    }
    pub fn event_action(
        &mut self,
        event: &Event,
        controls: &[Control],
        hits: &[ControlHit],
    ) -> Option<BlendAction> {
        if controls.is_empty() {
            return None;
        }
        self.focus = self.focus.min(controls.len() - 1);
        match event {
            Event::Key(key)
                if key.kind != KeyEventKind::Release
                    && !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                match key.code {
                    KeyCode::Tab | KeyCode::Down => {
                        self.focus = (self.focus + 1) % controls.len();
                        None
                    }
                    KeyCode::BackTab | KeyCode::Up => {
                        self.focus = (self.focus + controls.len() - 1) % controls.len();
                        None
                    }
                    KeyCode::Enter => controls
                        .get(self.focus)
                        .filter(|control| control.disabled.is_none())
                        .map(|control| control.action),
                    _ => controls
                        .iter()
                        .position(|control| crate::footer::key_matches(&control.key, key))
                        .and_then(|index| {
                            self.focus = index;
                            controls[index]
                                .disabled
                                .is_none()
                                .then_some(controls[index].action)
                        }),
                }
            }
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => hits
                .iter()
                .find(|hit| hit.rect.contains((mouse.column, mouse.row).into()))
                .and_then(|hit| {
                    self.focus = hit.index;
                    controls
                        .get(hit.index)
                        .filter(|control| control.disabled.is_none())
                        .map(|control| control.action)
                }),
            _ => None,
        }
    }
}

/// Row-derived full-label click targets, with keyboard focus scrolling the small
/// control list into view on short terminals. No hard-coded application coords.
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    controls: &[Control],
    focus: usize,
    theme: Theme,
) -> Vec<ControlHit> {
    let height = area.height as usize;
    if height == 0 || area.width == 0 {
        return vec![];
    }
    let visible_focus = focus.min(controls.len().saturating_sub(1));
    let first = visible_focus.saturating_add(1).saturating_sub(height);
    controls
        .iter()
        .enumerate()
        .skip(first)
        .take(height)
        .map(|(index, control)| {
            let rect = Rect::new(area.x, area.y + (index - first) as u16, area.width, 1);
            let mut style = theme.style(if control.disabled.is_none() {
                SemanticToken::Reference
            } else {
                SemanticToken::Muted
            });
            if index == focus {
                style = style.add_modifier(Modifier::REVERSED | Modifier::BOLD);
            }
            frame.render_widget(
                Paragraph::new(format!("[{}]", control.token(false))).style(style),
                rect,
            );
            ControlHit { rect, index }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::ThemeName, theme::ColorMode};
    use crossterm::event::{KeyEvent, MouseEvent};
    use ratatui::{backend::TestBackend, Terminal};

    #[test]
    fn collaboration_index_blend_controls_keyboard_mouse_and_disabled_scope_share_actions() {
        let mut state = BlendControls::default();
        let unavailable = state.controls(SearchMode::Ranked, false);
        let shortcut = Event::Key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));
        assert_eq!(state.event_action(&shortcut, &unavailable, &[]), None);
        assert!(!state.apply(BlendAction::Toggle, SearchMode::Ranked, false));
        let controls = state.controls(SearchMode::Ranked, true);
        let key = state.event_action(&shortcut, &controls, &[]).unwrap();
        let click = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 3,
            modifiers: KeyModifiers::NONE,
        });
        let mouse = state
            .event_action(
                &click,
                &controls,
                &[ControlHit {
                    rect: Rect::new(2, 3, 60, 1),
                    index: 0,
                }],
            )
            .unwrap();
        assert_eq!(key, mouse);
        assert!(state.apply(mouse, SearchMode::Ranked, true));
        assert!(state.policy.enabled);
        for mode in [SearchMode::Exact, SearchMode::Regex, SearchMode::GitHub] {
            assert!(state
                .controls(mode, true)
                .iter()
                .filter(|control| control.action != BlendAction::Close)
                .all(|control| control.disabled.is_some()));
            assert!(!state.apply(BlendAction::Toggle, mode, true));
        }
        assert!(state.apply(BlendAction::Toggle, SearchMode::Ranked, false));
        assert!(!state.policy.enabled);
    }

    #[test]
    fn collaboration_index_blend_controls_keep_one_enabled_source_and_bound_all_caps() {
        let mut state = BlendControls::default();
        assert!(state.apply(BlendAction::Toggle, SearchMode::Ranked, true));
        assert!(state.apply(
            BlendAction::Source(RankingSource::GithubCode),
            SearchMode::Ranked,
            true
        ));
        assert!(state.apply(
            BlendAction::Source(RankingSource::GithubIssues),
            SearchMode::Ranked,
            true
        ));
        assert!(!state.apply(
            BlendAction::Source(RankingSource::GithubPullRequests),
            SearchMode::Ranked,
            true
        ));
        assert_eq!(state.policy.sources.len(), 1);
        assert!(!state.apply(
            BlendAction::Source(RankingSource::LocalLexical),
            SearchMode::Ranked,
            true
        ));
        state.policy.per_source_results = 1;
        assert!(!state.apply(
            BlendAction::Adjust(LimitField::SourceResults, -1),
            SearchMode::Ranked,
            true
        ));
        state.policy.per_source_pages = 10;
        assert!(!state.apply(
            BlendAction::Adjust(LimitField::SourcePages, 1),
            SearchMode::Ranked,
            true
        ));
        state.policy.max_results = 4999;
        assert!(state.apply(
            BlendAction::Adjust(LimitField::FinalResults, 1),
            SearchMode::Ranked,
            true
        ));
        assert!(!state.apply(
            BlendAction::Adjust(LimitField::FinalResults, 1),
            SearchMode::Ranked,
            true
        ));
        assert!(state.policy.validate().is_ok());
        assert_eq!(
            serde_json::from_str::<BlendPolicy>("{}").unwrap(),
            BlendPolicy::default()
        );
    }

    #[test]
    fn collaboration_index_blend_controls_narrow_focus_scroll_rebuilds_visible_click_targets() {
        let mut state = BlendControls {
            focus: 10,
            ..Default::default()
        };
        let controls = state.controls(SearchMode::Ranked, true);
        let theme = Theme::new(ThemeName::NoColor, ColorMode::NoColor);
        for (width, height) in [(80, 24), (40, 8), (24, 3)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            let mut hits = vec![];
            terminal
                .draw(|frame| {
                    let area = frame.area();
                    hits = draw(frame, area, &controls, state.focus, theme)
                })
                .unwrap();
            assert!(hits.iter().any(|hit| hit.index == state.focus));
            let focused = hits.iter().find(|hit| hit.index == state.focus).unwrap();
            let visible = terminal.backend().buffer();
            let label = (0..width)
                .map(|x| visible[(x, focused.rect.y)].symbol())
                .collect::<String>();
            assert!(label.contains("Esc Close"));
            assert!(visible[(0, focused.rect.y)]
                .modifier
                .contains(Modifier::REVERSED | Modifier::BOLD));
            let click = Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 1,
                row: focused.rect.y,
                modifiers: KeyModifiers::NONE,
            });
            assert_eq!(
                state.event_action(&click, &controls, &hits),
                Some(BlendAction::Close)
            );
        }
    }
}
