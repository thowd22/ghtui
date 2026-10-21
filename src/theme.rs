//! Shared semantic styles; status meaning survives color capabilities and selection overlays.
use crate::config::ThemeName;
use crate::repository_view::SemanticRole;
use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    TrueColor,
    /// The xterm 256-color cube and gray ramp; palettes are matched to their nearest entry.
    Indexed256,
    /// The terminal's 16 named colors; roles keep their meaning through a fixed mapping.
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
        let term = term.unwrap_or_default().to_ascii_lowercase();
        let colorterm = colorterm.unwrap_or_default().to_ascii_lowercase();
        if colorterm == "truecolor"
            || colorterm == "24bit"
            || term.contains("truecolor")
            || term.contains("direct")
        {
            Self::TrueColor
        } else if term.contains("256color") || colorterm.contains("256") {
            Self::Indexed256
        } else {
            Self::LowColor
        }
    }
}

/// Converts any color to what `mode` can show. 16-color output keeps the hue family (red stays
/// red, pastel or not) rather than the nearest RGB point, which would turn pastels into white.
pub fn downsample(color: Color, mode: ColorMode) -> Color {
    let rgb = |color| match color {
        Color::Rgb(r, g, b) => Some((r, g, b)),
        Color::Indexed(index @ 16..=231) => {
            let n = index - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            Some((level(n / 36), level((n / 6) % 6), level(n % 6)))
        }
        Color::Indexed(index @ 232..=255) => {
            let gray = 8 + (index - 232) * 10;
            Some((gray, gray, gray))
        }
        _ => None,
    };
    match mode {
        ColorMode::NoColor => Color::Reset,
        ColorMode::TrueColor => color,
        ColorMode::Indexed256 => match color {
            Color::Rgb(r, g, b) => Color::Indexed(nearest_256(r, g, b)),
            other => other,
        },
        ColorMode::LowColor => match rgb(color) {
            Some((r, g, b)) => Color::Indexed(nearest_16(r, g, b)),
            None => color,
        },
    }
}

fn nearest_256(r: u8, g: u8, b: u8) -> u8 {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let distance = |(x, y, z): (u8, u8, u8)| {
        let d = |a: u8, b: u8| (i32::from(a) - i32::from(b)).pow(2);
        d(x, r) + d(y, g) + d(z, b)
    };
    let step = |v: u8| {
        (0..6)
            .min_by_key(|&i| (i32::from(LEVELS[i]) - i32::from(v)).abs())
            .unwrap_or(0)
    };
    let (ri, gi, bi) = (step(r), step(g), step(b));
    let cube = (LEVELS[ri], LEVELS[gi], LEVELS[bi]);
    let average = (u16::from(r) + u16::from(g) + u16::from(b)) / 3;
    let gray_index = (average.saturating_sub(3) / 10).min(23) as u8;
    let gray = 8 + gray_index * 10;
    if distance((gray, gray, gray)) < distance(cube) {
        232 + gray_index
    } else {
        16 + 36 * ri as u8 + 6 * gi as u8 + bi as u8
    }
}

fn nearest_16(r: u8, g: u8, b: u8) -> u8 {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let chroma = i32::from(max - min);
    if chroma < 40 {
        return match (u16::from(max) + u16::from(min)) / 2 {
            0..=63 => 0,
            64..=127 => 8,
            128..=199 => 7,
            _ => 15,
        };
    }
    let (r, g, b) = (i32::from(r), i32::from(g), i32::from(b));
    let hue = if max == r as u8 {
        (60 * (g - b) / chroma).rem_euclid(360)
    } else if max == g as u8 {
        60 * (b - r) / chroma + 120
    } else {
        60 * (r - g) / chroma + 240
    };
    // red, yellow, green, cyan, blue, magenta in ANSI index order.
    let base = [1, 3, 2, 6, 4, 5][(((hue + 30) / 60) % 6) as usize];
    base + if max > 200 { 8 } else { 0 }
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
    /// The view's hue for headings and active controls; unlike Focus it carries no underline.
    Accent,
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
            Self::Accent => "",
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
            Self::Accent => "heading",
            Self::Focus => "focused",
            Self::Selection => "selected",
        }
    }
}

impl SemanticToken {
    /// The one mapping from a GitHub check run, commit status, workflow run or job state to its
    /// semantic token, shared by Actions, PR Checks and the scoped PR resources so the same
    /// state never shows as passed in one view and waiting in another. A conclusion decides
    /// first (`error` is a commit status that errored); without one the live status does.
    /// Completed rows with an unknown conclusion and unknown states are muted, not waiting.
    pub fn ci_state(status: &str, conclusion: &str) -> Self {
        match conclusion.to_ascii_lowercase().as_str() {
            "success" => Self::Success,
            "failure" | "error" | "timed_out" | "startup_failure" | "action_required" => {
                Self::Failure
            }
            "cancelled" => Self::Cancelled,
            "skipped" | "neutral" | "stale" => Self::Muted,
            _ => match status.to_ascii_lowercase().as_str() {
                "in_progress" => Self::Running,
                "queued" | "waiting" | "pending" | "requested" | "expected" => Self::Pending,
                _ => Self::Muted,
            },
        }
    }
    /// The role that colors this token. Status tokens always pair the color with `symbol()`.
    pub fn role(self) -> Role {
        match self {
            Self::Normal => Role::Text,
            Self::Muted | Self::Pending | Self::Cancelled => Role::TextMuted,
            Self::Ancestor => Role::TextDim,
            Self::Added | Self::Success => Role::Success,
            Self::Removed | Self::Failure | Self::Conflict => Role::Failure,
            Self::Changed | Self::Running | Self::Diverged => Role::Warning,
            Self::Head => Role::Head,
            Self::Reference => Role::Branch,
            Self::Actor => Role::Text,
            Self::Accent | Self::Focus => Role::Accent,
            Self::Selection => Role::Selection,
        }
    }
    /// Upper-case word for a filled status badge in details and titles, e.g. ` ✓ PASSED `.
    pub fn badge_label(self) -> &'static str {
        match self {
            Self::Success => "PASSED",
            Self::Failure => "FAILURE",
            Self::Running => "RUNNING",
            Self::Pending => "QUEUED",
            Self::Cancelled => "CANCELLED",
            Self::Added => "ADDED",
            Self::Removed => "REMOVED",
            Self::Changed => "MODIFIED",
            Self::Conflict => "CONFLICT",
            _ => "",
        }
    }
}

/// What a piece of text is, independent of theme and terminal capability. Views style through
/// roles (or the `SemanticToken`s that map onto them), never raw colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The view hue: tab, headings, footer keys.
    Accent,
    Text,
    TextMuted,
    /// Decoration only (gutters, separators); never the sole carrier of information.
    TextDim,
    Border,
    BorderFocus,
    Selection,
    Success,
    Failure,
    Warning,
    Info,
    /// Commit SHAs.
    Commit,
    Branch,
    Tag,
    Remote,
    Head,
    /// Workflow events such as `push` or `workflow_dispatch`.
    Event,
    /// Counts and numbers such as `#42`.
    Number,
}

impl Role {
    pub const ALL: [Self; 18] = [
        Self::Accent,
        Self::Text,
        Self::TextMuted,
        Self::TextDim,
        Self::Border,
        Self::BorderFocus,
        Self::Selection,
        Self::Success,
        Self::Failure,
        Self::Warning,
        Self::Info,
        Self::Commit,
        Self::Branch,
        Self::Tag,
        Self::Remote,
        Self::Head,
        Self::Event,
        Self::Number,
    ];
}

/// Each view owns a hue for its tab, focused border, section headers and footer keys.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Hue {
    #[default]
    Mauve,
    Peach,
    Teal,
    Blue,
    Green,
}

impl Hue {
    pub fn for_view(view: crate::app::View) -> Self {
        match view {
            crate::app::View::Inbox => Self::Mauve,
            crate::app::View::Repository => Self::Peach,
            crate::app::View::Search => Self::Teal,
            crate::app::View::Actions => Self::Blue,
            crate::app::View::Dashboard => Self::Green,
        }
    }
}

/// A bundled truecolor palette as 0xRRGGBB values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub bg: u32,
    pub fg: u32,
    pub muted: u32,
    pub dim: u32,
    pub border: u32,
    pub selection: u32,
    pub blue: u32,
    pub green: u32,
    pub red: u32,
    pub yellow: u32,
    pub cyan: u32,
    pub magenta: u32,
    pub orange: u32,
    pub pink: u32,
    /// Failure text. Equal to `red` unless `red` misses 4.5:1 on the background.
    pub red_text: u32,
    /// Text drawn on filled badges; the background unless that misses 4.5:1 on a status fill.
    pub ink: u32,
}

pub const CATPPUCCIN_MOCHA: Palette = Palette {
    bg: 0x1e1e2e,
    fg: 0xcdd6f4,
    muted: 0xa6adc8,
    dim: 0x6c7086,
    border: 0x45475a,
    selection: 0x363a4f,
    blue: 0x89b4fa,
    green: 0xa6e3a1,
    red: 0xf38ba8,
    yellow: 0xf9e2af,
    cyan: 0x94e2d5,
    magenta: 0xcba6f7,
    orange: 0xfab387,
    pink: 0xf5c2e7,
    red_text: 0xf38ba8,
    ink: 0x1e1e2e,
};

pub const TOKYO_NIGHT: Palette = Palette {
    bg: 0x1a1b26,
    fg: 0xc0caf5,
    muted: 0x9aa5ce,
    dim: 0x565f89,
    border: 0x3b4261,
    selection: 0x283457,
    blue: 0x7aa2f7,
    green: 0x9ece6a,
    red: 0xf7768e,
    yellow: 0xe0af68,
    cyan: 0x7dcfff,
    magenta: 0xbb9af7,
    orange: 0xff9e64,
    pink: 0xff79c6,
    red_text: 0xf7768e,
    ink: 0x1a1b26,
};

pub const GRUVBOX_DARK: Palette = Palette {
    bg: 0x282828,
    fg: 0xebdbb2,
    muted: 0xa89984,
    dim: 0x665c54,
    border: 0x504945,
    selection: 0x3c3836,
    blue: 0x83a598,
    green: 0xb8bb26,
    red: 0xfb4934,
    yellow: 0xfabd2f,
    cyan: 0x8ec07c,
    magenta: 0xd3869b,
    orange: 0xfe8019,
    pink: 0xd3869b,
    // Bright red reaches only 4.3:1 on bg0, so failure text is tinted 12% toward fg.
    red_text: 0xf95b43,
    // Gruvbox's dark0_hard: the regular background reaches only 4.3:1 on the red badge.
    ink: 0x1d2021,
};

impl Palette {
    pub fn for_theme(name: ThemeName) -> Option<Self> {
        match name {
            ThemeName::CatppuccinMocha => Some(CATPPUCCIN_MOCHA),
            ThemeName::TokyoNight => Some(TOKYO_NIGHT),
            ThemeName::GruvboxDark => Some(GRUVBOX_DARK),
            ThemeName::Ansi16 | ThemeName::NoColor => None,
        }
    }
    pub fn hue(self, hue: Hue) -> u32 {
        match hue {
            Hue::Mauve => self.magenta,
            Hue::Peach => self.orange,
            Hue::Teal => self.cyan,
            Hue::Blue => self.blue,
            Hue::Green => self.green,
        }
    }
    pub fn role(self, role: Role, hue: Hue) -> u32 {
        match role {
            Role::Accent | Role::BorderFocus => self.hue(hue),
            Role::Text | Role::Selection => self.fg,
            Role::TextMuted => self.muted,
            Role::TextDim => self.dim,
            Role::Border => self.border,
            Role::Success => self.green,
            Role::Failure => self.red_text,
            Role::Warning | Role::Commit | Role::Tag => self.yellow,
            Role::Info | Role::Branch | Role::Remote => self.cyan,
            Role::Head | Role::Number => self.orange,
            Role::Event => self.pink,
        }
    }
    /// Graph lanes, in assignment order.
    pub fn lanes(self) -> [u32; 8] {
        [
            self.orange,
            self.cyan,
            self.magenta,
            self.green,
            self.blue,
            self.yellow,
            self.pink,
            self.red,
        ]
    }
}

/// Identity chips use the first seven graph lanes; the eighth is red.
pub const IDENTITY_HUES: usize = 7;

pub fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

fn ansi16_hue(hue: Hue) -> Color {
    match hue {
        Hue::Mauve => Color::Magenta,
        Hue::Peach => Color::LightRed,
        Hue::Teal => Color::Cyan,
        Hue::Blue => Color::Blue,
        Hue::Green => Color::Green,
    }
}

fn ansi16_role(role: Role, hue: Hue) -> Color {
    match role {
        Role::Accent | Role::BorderFocus => ansi16_hue(hue),
        Role::Text | Role::Selection => Color::Reset,
        Role::TextMuted => Color::Gray,
        Role::TextDim | Role::Border => Color::DarkGray,
        Role::Success => Color::Green,
        Role::Failure => Color::Red,
        Role::Warning | Role::Commit | Role::Tag => Color::Yellow,
        Role::Info | Role::Branch | Role::Remote => Color::Cyan,
        Role::Head | Role::Number => Color::LightRed,
        Role::Event => Color::LightMagenta,
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
    pub hue: Hue,
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
            hue: Hue::default(),
        }
    }
    pub fn from_settings(index: usize, no_color: bool) -> Self {
        Self::new(
            ThemeName::from_index(index),
            if no_color {
                ColorMode::NoColor
            } else {
                ColorMode::TrueColor
            },
        )
    }
    pub fn from_capability(index: usize, mode: ColorMode) -> Self {
        Self::new(ThemeName::from_index(index), mode)
    }
    /// The same theme with the hue the given view owns.
    pub fn with_view(self, view: crate::app::View) -> Self {
        Self {
            hue: Hue::for_view(view),
            ..self
        }
    }
    /// The palette color behind a hue, after downsampling (Reset when colorless).
    pub fn hue(self, hue: Hue) -> Color {
        match self.mode {
            ColorMode::NoColor => Color::Reset,
            _ => self.color(Role::Accent, hue),
        }
    }
    fn palette(self) -> Option<Palette> {
        match self.mode {
            ColorMode::NoColor | ColorMode::LowColor => None,
            _ => Palette::for_theme(self.name),
        }
    }
    fn color(self, role: Role, hue: Hue) -> Color {
        match self.palette() {
            Some(palette) => downsample(rgb(palette.role(role, hue)), self.mode),
            None => ansi16_role(role, hue),
        }
    }
    /// RGB the image canvas blends transparent pixels onto.
    pub fn backdrop(self) -> (u8, u8, u8) {
        let bg = Palette::for_theme(self.name).map_or(0, |palette| palette.bg);
        ((bg >> 16) as u8, (bg >> 8) as u8, bg as u8)
    }
    pub fn base(self) -> Style {
        match (self.mode, self.palette()) {
            (ColorMode::NoColor, _) => Style::default(),
            (_, Some(palette)) => Style::default()
                .fg(downsample(rgb(palette.fg), self.mode))
                .bg(downsample(rgb(palette.bg), self.mode)),
            // 16 colors use the terminal's own foreground and background.
            (_, None) => Style::default().fg(Color::Reset).bg(Color::Reset),
        }
    }
    /// Style for a role. Without color, roles that must stand out keep a non-color modifier.
    pub fn role(self, role: Role) -> Style {
        let style = self.base();
        let emphasis = match role {
            Role::Accent | Role::BorderFocus | Role::Head | Role::Branch | Role::Failure => {
                Modifier::BOLD
            }
            _ => Modifier::empty(),
        };
        if self.mode == ColorMode::NoColor {
            return match role {
                Role::Selection => style.add_modifier(Modifier::REVERSED),
                Role::TextDim => style.add_modifier(Modifier::DIM),
                _ => style.add_modifier(emphasis),
            };
        }
        match role {
            Role::Selection => match self.palette() {
                Some(palette) => style.bg(downsample(rgb(palette.selection), self.mode)),
                None => style.add_modifier(Modifier::REVERSED),
            },
            _ => style.fg(self.color(role, self.hue)).add_modifier(emphasis),
        }
    }
    pub fn style(self, token: SemanticToken) -> Style {
        let style = self.base();
        let modifier = match token {
            SemanticToken::Focus => Modifier::UNDERLINED,
            SemanticToken::Selection => Modifier::REVERSED,
            SemanticToken::Head
            | SemanticToken::Reference
            | SemanticToken::Conflict
            | SemanticToken::Actor
            | SemanticToken::Accent => Modifier::BOLD,
            SemanticToken::Failure if self.mode == ColorMode::NoColor => Modifier::BOLD,
            _ => Modifier::empty(),
        };
        if self.mode == ColorMode::NoColor {
            return style.add_modifier(modifier);
        }
        // Selection keeps reversing its text so every token stays legible inside a selected row.
        let role = match token {
            SemanticToken::Selection => Role::Text,
            token => token.role(),
        };
        style.fg(self.color(role, self.hue)).add_modifier(modifier)
    }
    pub fn selected(self, token: SemanticToken) -> Style {
        self.style(token)
            .add_modifier(Modifier::REVERSED | Modifier::BOLD)
    }
    /// A filled status badge for details and titles; list rows use the colored glyph instead.
    pub fn badge(self, token: SemanticToken) -> Style {
        if self.mode == ColorMode::NoColor {
            return self
                .base()
                .add_modifier(Modifier::REVERSED | Modifier::BOLD);
        }
        let muted = matches!(token.role(), Role::TextMuted | Role::TextDim | Role::Text);
        let (fill, ink) = match self.palette() {
            Some(palette) => (
                rgb(if muted {
                    palette.border
                } else if token.role() == Role::Failure {
                    palette.red
                } else {
                    palette.role(token.role(), self.hue)
                }),
                rgb(if muted { palette.fg } else { palette.ink }),
            ),
            None => (
                if muted {
                    Color::DarkGray
                } else {
                    ansi16_role(token.role(), self.hue)
                },
                if muted { Color::White } else { Color::Black },
            ),
        };
        Style::default()
            .fg(downsample(ink, self.mode))
            .bg(downsample(fill, self.mode))
            .add_modifier(Modifier::BOLD)
    }
    /// Ink on a solid view hue: the logo chip and the active view tab.
    pub fn filled(self, hue: Hue) -> Style {
        self.fill(
            self.palette().map(|palette| palette.hue(hue)),
            ansi16_hue(hue),
        )
    }
    /// Ink on one of the identity hues. Red is left out so a person never reads as a failure.
    pub fn identity_chip(self, slot: usize) -> Style {
        if self.mode == ColorMode::NoColor {
            return self.base().add_modifier(Modifier::BOLD);
        }
        let slot = slot % IDENTITY_HUES;
        self.fill(
            self.palette().map(|palette| palette.lanes()[slot]),
            [
                Color::LightRed,
                Color::Cyan,
                Color::Magenta,
                Color::Green,
                Color::Blue,
                Color::Yellow,
                Color::LightMagenta,
            ][slot],
        )
    }
    /// Ink on a role's color: History ref chips (Head, Branch, Remote, Tag). Without color it
    /// is reverse video, like every other filled chip.
    pub fn chip(self, role: Role) -> Style {
        self.fill(
            self.palette().map(|palette| palette.role(role, self.hue)),
            ansi16_role(role, self.hue),
        )
    }
    /// A role's ink on the raised surface (the border color): remote-branch chips, so they
    /// read apart from the filled local-branch chips that share the hue.
    pub fn soft_chip(self, role: Role) -> Style {
        if self.mode == ColorMode::NoColor {
            return self
                .base()
                .add_modifier(Modifier::REVERSED | Modifier::BOLD);
        }
        let surface = match self.palette() {
            Some(palette) => downsample(rgb(palette.border), self.mode),
            None => Color::DarkGray,
        };
        self.role(role).bg(surface).add_modifier(Modifier::BOLD)
    }
    /// Backgrounds of filled chips (identity, ref and soft chips) that keep their fill on a
    /// selected row.
    pub fn chip_fills(self) -> Vec<Color> {
        if self.mode == ColorMode::NoColor {
            return Vec::new();
        }
        let mut fills: Vec<Color> = (0..IDENTITY_HUES)
            .filter_map(|slot| self.identity_chip(slot).bg)
            .collect();
        fills.extend(
            [Role::Head, Role::Branch, Role::Remote, Role::Tag]
                .into_iter()
                .filter_map(|role| self.chip(role).bg)
                .chain(self.soft_chip(Role::Remote).bg),
        );
        fills
    }
    fn fill(self, hex: Option<u32>, named: Color) -> Style {
        if self.mode == ColorMode::NoColor {
            return self
                .base()
                .add_modifier(Modifier::REVERSED | Modifier::BOLD);
        }
        let (fill, ink) = match (hex, self.palette()) {
            (Some(hex), Some(palette)) => (
                downsample(rgb(hex), self.mode),
                downsample(rgb(palette.ink), self.mode),
            ),
            _ => (named, Color::Black),
        };
        Style::default()
            .fg(ink)
            .bg(fill)
            .add_modifier(Modifier::BOLD)
    }
    /// Badge text, always with the glyph so the status survives any color capability.
    pub fn badge_text(token: SemanticToken) -> String {
        format!(" {} {} ", token.symbol(), token.badge_label())
    }
    /// Graph lane styles in assignment order; lanes trace parent paths, not branch identity.
    pub fn lanes(self) -> [Style; 8] {
        if self.mode == ColorMode::NoColor {
            return [self.style(SemanticToken::Ancestor); 8];
        }
        let colors = match self.palette() {
            Some(palette) => palette.lanes().map(|hex| downsample(rgb(hex), self.mode)),
            None => [
                Color::LightRed,
                Color::Cyan,
                Color::Magenta,
                Color::Green,
                Color::Blue,
                Color::Yellow,
                Color::LightMagenta,
                Color::Red,
            ],
        };
        colors.map(|color| self.base().fg(color))
    }
    pub fn graph(self, slot: usize) -> Style {
        self.lanes()[slot % 8]
    }
    pub fn legend(self) -> Vec<String> {
        let mut rows=vec!["Git graph legend — colors trace parent paths, never permanent branch identities".into(),"● commit: a saved version; │ parent path; ╲ merge: joining histories".into(),"Names in brackets are branches/tags pointing to a saved version; HEAD is where you are".into(),"A boundary means earlier versions are not loaded or available in a shallow clone".into(),"A bold commit ID marks the selected commit and the commits it descends from".into()];
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

#[cfg(test)]
#[path = "theme_audit_v64_tests.rs"]
mod audit_v64_tests;
#[cfg(test)]
#[path = "theme_tests.rs"]
mod tests;
