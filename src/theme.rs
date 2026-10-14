//! Shared semantic styles; status meaning survives color capabilities and selection overlays.
use crate::config::ThemeName;
use crate::repository_view::SemanticRole;
use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    TrueColor,
    LowColor,
    NoColor,
}

impl ColorMode {
    /// Capture capability once when creating/updating application state, never while drawing.
    pub fn detect() -> Self {
        Self::for_terminal(
            std::env::var("TERM").ok().as_deref(),
            std::env::var("COLORTERM").ok().as_deref(),
            std::env::var_os("NO_COLOR").is_some(),
        )
    }
    /// Capability inputs are captured outside drawing; no terminal probing or I/O is needed.
    pub fn for_terminal(term: Option<&str>, colorterm: Option<&str>, no_color: bool) -> Self {
        if no_color || term.is_some_and(|term| term.eq_ignore_ascii_case("dumb")) {
            return Self::NoColor;
        }
        if colorterm.is_some_and(|value| {
            value.eq_ignore_ascii_case("truecolor") || value.eq_ignore_ascii_case("24bit")
        }) || term.is_some_and(|term| {
            let term = term.to_ascii_lowercase();
            term.contains("truecolor") || term.contains("direct")
        }) {
            Self::TrueColor
        } else {
            Self::LowColor
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticToken {
    Normal,
    Muted,
    Added,
    Removed,
    Changed,
    Conflict,
    Head,
    Reference,
    Ancestor,
    Diverged,
    Success,
    Failure,
    Running,
    Pending,
    Cancelled,
    Actor,
    Focus,
    Selection,
}

impl SemanticToken {
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Added => "+",
            Self::Removed => "−",
            Self::Changed => "~",
            Self::Conflict => "!",
            Self::Head => "●",
            Self::Reference => "◆",
            Self::Ancestor => "│",
            Self::Diverged => "⑂",
            Self::Success => "✓",
            Self::Failure => "✗",
            Self::Running => "↻",
            Self::Pending => "…",
            Self::Cancelled => "⊘",
            Self::Actor => "@",
            Self::Focus | Self::Selection => ">",
            Self::Normal | Self::Muted => "",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Muted => "unavailable or incomplete",
            Self::Added => "added",
            Self::Removed => "removed",
            Self::Changed => "modified",
            Self::Conflict => "conflict",
            Self::Head => "HEAD / current saved version",
            Self::Reference => "branch or tag name",
            Self::Ancestor => "parent relationship",
            Self::Diverged => "histories diverged",
            Self::Success => "passed",
            Self::Failure => "failed",
            Self::Running => "running",
            Self::Pending => "waiting",
            Self::Cancelled => "cancelled",
            Self::Actor => "actor",
            Self::Focus => "focused",
            Self::Selection => "selected",
        }
    }
}

impl From<SemanticRole> for SemanticToken {
    fn from(role: SemanticRole) -> Self {
        match role {
            SemanticRole::Normal => Self::Normal,
            SemanticRole::Added => Self::Added,
            SemanticRole::Removed => Self::Removed,
            SemanticRole::Changed => Self::Changed,
            SemanticRole::Conflict => Self::Conflict,
            SemanticRole::Head => Self::Head,
            SemanticRole::Reference => Self::Reference,
            SemanticRole::Boundary => Self::Muted,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub name: ThemeName,
    pub mode: ColorMode,
}

impl Theme {
    pub fn new(name: ThemeName, mode: ColorMode) -> Self {
        Self {
            name,
            mode: if name == ThemeName::NoColor {
                ColorMode::NoColor
            } else {
                mode
            },
        }
    }
    pub fn from_settings(index: usize, no_color: bool) -> Self {
        Self::new(
            [ThemeName::Dark, ThemeName::Light, ThemeName::NoColor][index % 3],
            if no_color {
                ColorMode::NoColor
            } else {
                ColorMode::TrueColor
            },
        )
    }
    pub fn from_capability(index: usize, mode: ColorMode) -> Self {
        Self::new(
            [ThemeName::Dark, ThemeName::Light, ThemeName::NoColor][index % 3],
            mode,
        )
    }
    pub fn base(self) -> Style {
        if self.mode == ColorMode::NoColor {
            return Style::default();
        }
        if self.name == ThemeName::Light {
            Style::default().fg(Color::Black).bg(Color::White)
        } else {
            Style::default().fg(Color::White).bg(Color::Black)
        }
    }
    pub fn style(self, token: SemanticToken) -> Style {
        let style = self.base();
        if self.mode == ColorMode::NoColor {
            return match token {
                SemanticToken::Head
                | SemanticToken::Reference
                | SemanticToken::Conflict
                | SemanticToken::Failure
                | SemanticToken::Actor => style.add_modifier(Modifier::BOLD),
                SemanticToken::Focus => style.add_modifier(Modifier::UNDERLINED),
                SemanticToken::Selection => style.add_modifier(Modifier::REVERSED),
                _ => style,
            };
        }
        let light = self.name == ThemeName::Light;
        let color = match token {
            SemanticToken::Added | SemanticToken::Success => {
                if light {
                    Color::Rgb(0, 96, 45)
                } else {
                    Color::Rgb(90, 224, 144)
                }
            }
            SemanticToken::Removed | SemanticToken::Failure => {
                if light {
                    Color::Rgb(165, 25, 35)
                } else {
                    Color::Rgb(255, 125, 130)
                }
            }
            SemanticToken::Changed | SemanticToken::Running => {
                if light {
                    Color::Rgb(0, 80, 145)
                } else {
                    Color::Rgb(120, 185, 255)
                }
            }
            SemanticToken::Conflict | SemanticToken::Diverged => {
                if light {
                    Color::Rgb(133, 35, 133)
                } else {
                    Color::Rgb(245, 140, 240)
                }
            }
            SemanticToken::Head | SemanticToken::Actor => {
                if light {
                    Color::Rgb(0, 92, 104)
                } else {
                    Color::Rgb(95, 225, 235)
                }
            }
            SemanticToken::Reference | SemanticToken::Pending => {
                if light {
                    Color::Rgb(130, 75, 0)
                } else {
                    Color::Rgb(245, 205, 90)
                }
            }
            SemanticToken::Muted | SemanticToken::Cancelled => {
                if light {
                    Color::Rgb(90, 90, 90)
                } else {
                    Color::Rgb(165, 165, 165)
                }
            }
            _ => {
                if light {
                    Color::Black
                } else {
                    Color::White
                }
            }
        };
        let color = if self.mode == ColorMode::LowColor {
            match token {
                SemanticToken::Added | SemanticToken::Success => Color::Green,
                SemanticToken::Removed | SemanticToken::Failure => Color::Red,
                SemanticToken::Changed | SemanticToken::Running => Color::Blue,
                SemanticToken::Conflict | SemanticToken::Diverged => Color::Magenta,
                SemanticToken::Head | SemanticToken::Actor => Color::Cyan,
                SemanticToken::Reference | SemanticToken::Pending => Color::Yellow,
                SemanticToken::Muted | SemanticToken::Cancelled => Color::Gray,
                _ => {
                    if light {
                        Color::Black
                    } else {
                        Color::White
                    }
                }
            }
        } else {
            color
        };
        let style = style.fg(color);
        match token {
            SemanticToken::Focus => style.add_modifier(Modifier::UNDERLINED),
            SemanticToken::Selection => style.add_modifier(Modifier::REVERSED),
            SemanticToken::Head
            | SemanticToken::Reference
            | SemanticToken::Conflict
            | SemanticToken::Actor => style.add_modifier(Modifier::BOLD),
            _ => style,
        }
    }
    pub fn selected(self, token: SemanticToken) -> Style {
        self.style(token)
            .add_modifier(Modifier::REVERSED | Modifier::BOLD)
    }
    pub fn graph(self, slot: usize) -> Style {
        if self.mode == ColorMode::NoColor {
            return self.style(SemanticToken::Ancestor);
        }
        if self.mode == ColorMode::TrueColor {
            return self.style(
                [
                    SemanticToken::Head,
                    SemanticToken::Reference,
                    SemanticToken::Conflict,
                    SemanticToken::Added,
                    SemanticToken::Changed,
                    SemanticToken::Removed,
                    SemanticToken::Muted,
                    SemanticToken::Normal,
                ][slot % 8],
            );
        }
        let colors = if self.name == ThemeName::Light {
            [
                Color::Cyan,
                Color::Magenta,
                Color::Blue,
                Color::Green,
                Color::Red,
                Color::Black,
                Color::DarkGray,
                Color::Blue,
            ]
        } else {
            [
                Color::LightCyan,
                Color::LightYellow,
                Color::LightMagenta,
                Color::LightGreen,
                Color::LightBlue,
                Color::LightRed,
                Color::White,
                Color::LightMagenta,
            ]
        };
        self.base().fg(colors[slot % colors.len()])
    }
    pub fn legend(self) -> Vec<String> {
        let mut rows=vec!["Git graph legend — colors trace parent paths, never permanent branch identities".into(),"● commit: a saved version; │ parent path; ╲ merge: joining histories".into(),"Names in brackets are branches/tags pointing to a saved version; HEAD is where you are".into(),"A boundary means earlier versions are not loaded or available in a shallow clone".into()];
        rows.extend(
            [
                SemanticToken::Added,
                SemanticToken::Removed,
                SemanticToken::Changed,
                SemanticToken::Conflict,
                SemanticToken::Success,
                SemanticToken::Failure,
                SemanticToken::Running,
                SemanticToken::Pending,
                SemanticToken::Cancelled,
            ]
            .into_iter()
            .map(|token| format!("{} {}", token.symbol(), token.label())),
        );
        rows
    }
}
