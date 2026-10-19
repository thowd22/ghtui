//! Shared chrome: the merged header row, rounded pane frames with breadcrumb titles, section
//! headers and label/value rows. Focus never relies on color alone: a focused pane also gets a
//! bold title with the `▶` marker, and the keyboard-focused header control stays underlined.
use crate::app::{safe_text, Action, App, HitRegion, View};
use crate::config::Density;
use crate::footer::{ControlGroup, KeyControl};
use crate::identity::{ActorIdentity, ActorKind, InitialsChip};
use crate::theme::{Hue, Role, Theme};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};
use unicode_segmentation::UnicodeSegmentation;

/// The non-color focus marker in a focused pane's title.
pub const FOCUS_MARKER: &str = "▶";
/// Separates breadcrumb levels in pane titles.
pub const CRUMB_SEPARATOR: &str = " › ";
const LOGO: &str = " ghtui ";
/// The Repository Source identity token in the content pane's top border: its key (`I`, the
/// Repository key that opens the same explanation) and its full and compact labels.
pub const SOURCE_KEY: &str = "I";
const SOURCE_LABEL: &str = "Source identity";
const SOURCE_COMPACT: &str = "Source";
/// Below this width chrome tokens use compact labels, as the footer key bar does.
const COMPACT_WIDTH: u16 = 100;

/// The title text of a pane: breadcrumbs joined by `›`, with the focus marker when focused.
pub fn crumb_title(crumbs: &[&str], focused: bool) -> String {
    let joined = crumbs
        .iter()
        .map(|crumb| safe_text(crumb))
        .collect::<Vec<_>>()
        .join(CRUMB_SEPARATOR);
    if focused {
        format!(" {FOCUS_MARKER} {joined} ")
    } else {
        format!(" {joined} ")
    }
}

/// Border and title styles for a pane in the given focus state.
pub fn pane_styles(theme: Theme, focused: bool) -> (Style, Style) {
    if focused {
        (
            theme.role(Role::BorderFocus),
            theme.role(Role::Accent).add_modifier(Modifier::BOLD),
        )
    } else {
        (theme.role(Role::Border), theme.role(Role::TextMuted))
    }
}

/// A rounded pane whose top border reads `╭─ ▶ View › Detail ─…╮`.
pub fn pane_block(theme: Theme, crumbs: &[&str], focused: bool) -> Block<'static> {
    let (border, title) = pane_styles(theme, focused);
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border)
        .style(theme.base())
        .title(Line::from(vec![
            Span::styled("─", border),
            Span::styled(crumb_title(crumbs, focused), title),
        ]))
}

/// A modal overlay frame: the same rounded, focused `╭─ ▶ Title ─╮` frame as a focused pane,
/// since an overlay owns input while it is open.
pub fn overlay_block(theme: Theme, title: &str) -> Block<'static> {
    pane_block(theme, &[title.trim()], true)
}

/// Where a centered modal overlay (Help, Commands) may draw: inside the content pane, one
/// column clear of its side borders and below its top border, so the overlay frame never sits
/// on the pane frame (`╭─╭─`). When the pane is too small to hold a usable overlay, the old
/// whole-screen inset keeps Close reachable.
pub fn overlay_region(screen: Rect, pane: Rect) -> Rect {
    let region = pane.inner(ratatui::layout::Margin::new(2, 1));
    if region.height >= 12 && region.width >= 24 {
        region
    } else {
        screen.inner(ratatui::layout::Margin::new(2, 2))
    }
}

/// Blanks the one content column between an overlay `rect` and the pane border when the
/// overlay fills its `region` edge, so no clipped fragments of the view peek out beside it.
pub fn clear_overlay_gutter(frame: &mut Frame, theme: Theme, region: Rect, rect: Rect) {
    let screen = frame.area();
    for (flush, x) in [
        (
            rect.x == region.x && region.x > screen.x,
            region.x.saturating_sub(1),
        ),
        (
            rect.right() == region.right() && region.right() < screen.right(),
            region.right(),
        ),
    ] {
        if flush && !rect.is_empty() {
            let column = Rect::new(x, rect.y, 1, rect.height).intersection(screen);
            frame.render_widget(ratatui::widgets::Clear, column);
            frame.render_widget(Paragraph::new("").style(theme.base()), column);
        }
    }
}

/// Adds right-aligned provenance or context to a pane's top border when it fits in `budget`.
pub fn with_right_title(
    block: Block<'static>,
    theme: Theme,
    focused: bool,
    text: &str,
    budget: u16,
) -> Block<'static> {
    // Two cells of border stay between the titles, plus the padding around the text.
    let width = budget.saturating_sub(4);
    if width < 8 || text.is_empty() {
        return block;
    }
    let (border, _) = pane_styles(theme, focused);
    let text = compact_scope(&safe_text(text), width);
    block.title(
        Line::from(vec![
            Span::styled(format!(" {text} "), theme.role(Role::TextMuted)),
            Span::styled("─", border),
        ])
        .right_aligned(),
    )
}

/// Fits a ` / `-separated scope into `width` cells. The first two levels (host and account,
/// such as `github.com / @octocat`) stay whole when they take at most two thirds of the room; the
/// rest keeps its most specific end, such as the head SHA, around one ellipsis.
pub fn compact_scope(text: &str, width: u16) -> String {
    let cells = usize::from(width);
    if crate::editor::display_width(text) <= cells {
        return text.to_owned();
    }
    let lead = text
        .match_indices(" / ")
        .nth(1)
        .map(|(at, _)| &text[..at])
        .filter(|lead| crate::editor::display_width(lead) * 3 <= cells * 2);
    let Some(lead) = lead else {
        return crate::local_identity_readability_v1::compact_context(text, width);
    };
    let lead = format!("{lead} / …");
    // Restoring a whole ` / ` separator can add a cell, so the tail shrinks until it all fits.
    let mut budget = cells.saturating_sub(crate::editor::display_width(&lead));
    loop {
        let scope = scope_with_tail(text, lead.clone(), budget);
        if budget == 0 || crate::editor::display_width(&scope) <= cells {
            return scope;
        }
        budget -= 1;
    }
}

fn scope_with_tail(text: &str, lead: String, budget: usize) -> String {
    let mut tail = Vec::new();
    let mut used = 0;
    for grapheme in text.graphemes(true).rev() {
        let next = crate::editor::grapheme_width(grapheme);
        if used + next > budget {
            break;
        }
        tail.push(grapheme);
        used += next;
    }
    let tail = tail.into_iter().rev().collect::<String>();
    // A tail that starts inside a level is never cut mid-word when a whole level follows: the
    // broken level keeps its whole path segments, so an Inbox title reads `…/repo / My PRs` and
    // still names the repository, or is dropped when it has none (`… / run 7 / <sha>` rather
    // than `…wner / run 7 / <sha>`). A tail cut inside a separator starts at the next level.
    if let Some(rest) = tail.strip_prefix(" / ").or_else(|| tail.strip_prefix("/ ")) {
        return format!("{lead} / {rest}");
    }
    match tail.find(" / ") {
        Some(at) if !text.ends_with(&format!(" / {tail}")) && at + 3 < tail.len() => {
            match tail[..at].find('/') {
                Some(slash) => lead + &tail[slash..],
                None => format!("{lead} / {}", &tail[at + 3..]),
            }
        }
        _ => lead + &tail,
    }
}

/// Ends `text` with `…` when it is wider than `width` cells.
pub fn fit(text: &str, width: usize) -> String {
    if crate::editor::display_width(text) <= width {
        return text.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    let mut output = String::new();
    let mut cells = 0;
    for grapheme in text.graphemes(true) {
        let next = crate::editor::grapheme_width(grapheme);
        if cells + next + 1 > width {
            break;
        }
        output.push_str(grapheme);
        cells += next;
    }
    output.push('…');
    output
}

/// A section header: the title in the view hue on the left, provenance on the right. When both
/// do not fit, the provenance wins: freshness, coverage and partial states are never cut for a
/// heading. `detail_role` is TextMuted for settled provenance and Warning for partial data.
pub fn section_header(
    theme: Theme,
    title: &str,
    detail: &str,
    detail_role: Role,
    width: u16,
) -> Line<'static> {
    let width = usize::from(width);
    let title = title.to_uppercase();
    let detail = safe_text(detail);
    let detail_width = crate::editor::display_width(&detail);
    let title_width = crate::editor::display_width(&title);
    let heading = theme.role(Role::Accent).add_modifier(Modifier::BOLD);
    let detail_style = theme.role(detail_role);
    if detail.is_empty() {
        return Line::from(Span::styled(fit(&title, width), heading));
    }
    if title_width + 2 + detail_width > width {
        return Line::from(Span::styled(fit(&detail, width), detail_style));
    }
    let gap = width - title_width - detail_width;
    Line::from(vec![
        Span::styled(title, heading),
        Span::raw(" ".repeat(gap)),
        Span::styled(detail, detail_style),
    ])
}

/// A label/value row: the label muted and padded to `label_width`, the value in body text.
pub fn label_value(theme: Theme, label: &str, value: &str, label_width: usize) -> Line<'static> {
    let label = fit(label, label_width);
    let pad = label_width.saturating_sub(crate::editor::display_width(&label));
    Line::from(vec![
        Span::styled(
            format!("{label}{} ", " ".repeat(pad)),
            theme.role(Role::TextMuted),
        ),
        Span::styled(safe_text(value), theme.role(Role::Text)),
    ])
}

fn tab_label(app: &App, index: usize, view: View) -> (String, String) {
    (
        app.shortcut(["inbox", "repository", "search", "actions", "dashboard"][index])
            .to_owned(),
        view.label().to_owned(),
    )
}

fn tabs_width(app: &App) -> u16 {
    let tabs: usize = View::ALL
        .into_iter()
        .enumerate()
        .map(|(index, view)| {
            let (key, label) = tab_label(app, index, view);
            crate::editor::display_width(&format!(" {key} {label} ")) + 1
        })
        .sum();
    (LOGO.len() + 1 + tabs).min(usize::from(u16::MAX)) as u16
}

fn source_control(app: &App) -> bool {
    app.view == View::Repository && app.repository_source_identity.is_some()
}

/// A configured shortcut as a key-bar key token: `alt+left` reads `Alt-←`, `ctrl+b` reads
/// `Ctrl-b`, `f5` reads `F5` and a single character stays as it is.
pub fn key_token(binding: &str) -> String {
    let (modifiers, key) = match binding.strip_suffix("++") {
        Some(modifiers) => (modifiers, "+"),
        None => match binding.rsplit_once('+') {
            Some((modifiers, key)) if !key.is_empty() => (modifiers, key),
            _ => ("", binding),
        },
    };
    let key = match key {
        "left" => "←".to_owned(),
        "right" => "→".to_owned(),
        "up" => "↑".to_owned(),
        "down" => "↓".to_owned(),
        "pageup" => "PgUp".to_owned(),
        "pagedown" => "PgDn".to_owned(),
        key if key.chars().count() == 1 => key.to_owned(),
        key if key.len() > 1
            && key.starts_with('f')
            && key[1..].chars().all(|c| c.is_ascii_digit()) =>
        {
            key.to_uppercase()
        }
        key => {
            let mut chars = key.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        }
    };
    modifiers
        .split('+')
        .filter(|part| !part.is_empty())
        .map(|part| match part {
            "ctrl" => "Ctrl",
            "alt" => "Alt",
            "shift" => "Shift",
            other => other,
        })
        .chain(std::iter::once(key.as_str()))
        .collect::<Vec<_>>()
        .join("-")
}

/// A key-bar style token: the key in the accent hue and bold, the label muted. A keyboard
/// focused token is underlined, as in the footer.
pub fn key_token_line(theme: Theme, key: &str, label: &str, focused: bool) -> Line<'static> {
    let mut spans = Vec::new();
    if !key.is_empty() {
        spans.push(Span::styled(
            safe_text(key),
            theme.role(Role::Accent).add_modifier(Modifier::BOLD),
        ));
        if !label.is_empty() {
            spans.push(Span::styled(" ", theme.role(Role::TextMuted)));
        }
    }
    if !label.is_empty() {
        spans.push(Span::styled(safe_text(label), theme.role(Role::TextMuted)));
    }
    // Underline each span, so the focus survives when the spans join another title line.
    if focused {
        for span in &mut spans {
            span.style = span.style.add_modifier(Modifier::UNDERLINED);
        }
    }
    Line::from(spans)
}

/// Cells the left (breadcrumb) title of the main pane takes, with the focus marker, plus the
/// border cell before it.
fn main_title_width(app: &App) -> u16 {
    let title = format!("{} • focused view", app.view.label());
    crate::editor::display_width(&crumb_title(&[&title], true)) as u16 + 1
}

/// Where the Repository `I Source identity` token sits: at the right end of the content
/// pane's top border (`─ Local …/main/abc ─ I Source identity ─╮`), like the key hints pane
/// titles carry. The full label at 100 columns and wider while the scope keeps room, else the
/// compact `I Source`; None when even that does not fit beside the pane title.
pub fn source_token(app: &App, pane: Rect) -> Option<(Rect, &'static str)> {
    if !source_control(app) || pane.height == 0 {
        return None;
    }
    let budget = pane
        .width
        .saturating_sub(2)
        .saturating_sub(main_title_width(app));
    let need = |label: &str| (SOURCE_KEY.len() + 1 + label.len()) as u16;
    // ` token ─` closes the border; the scope title keeps a dozen cells on its left.
    let label = if pane.width >= COMPACT_WIDTH && need(SOURCE_LABEL) + 3 + 16 <= budget {
        SOURCE_LABEL
    } else if need(SOURCE_COMPACT) + 3 <= budget {
        SOURCE_COMPACT
    } else {
        return None;
    };
    let width = need(label);
    Some((
        Rect::new(pane.right().saturating_sub(3 + width), pane.y, width, 1),
        label,
    ))
}

/// Header rows: one whenever the logo, tabs and at least the compact Back token fit (every
/// width of 80 columns and more); else two, with Back and the crumbs on the second row.
pub fn header_rows(app: &App, width: u16) -> u16 {
    let needed = tabs_width(app).saturating_add(crate::navigation_state::NAVIGATION_MIN_WIDTH);
    if needed <= width {
        1
    } else {
        2
    }
}

/// The account chip: initials in the account's identity hue, then `login · host`.
fn account_line(app: &App, theme: Theme) -> Option<(Line<'static>, u16)> {
    let github = app.github.as_ref()?;
    github.identity.account_id?;
    let identity = ActorIdentity {
        host: Some(github.identity.host.clone()),
        login: Some(github.identity.login.clone()),
        name: String::new(),
        email: None,
        avatar_url: None,
        kind: ActorKind::User,
        git_role: None,
        host_associated: true,
    };
    let chip = InitialsChip::text(&identity, theme.mode);
    let style = if theme.mode == crate::theme::ColorMode::NoColor {
        theme.role(Role::Text).add_modifier(Modifier::BOLD)
    } else {
        theme.identity_chip(InitialsChip::slot(&identity))
    };
    let account = format!(
        " {} · {} ",
        safe_text(&github.identity.login),
        safe_text(&github.identity.host)
    );
    let width = crate::editor::display_width(&chip) + crate::editor::display_width(&account);
    Some((
        Line::from(vec![
            Span::styled(chip, style),
            Span::styled(account, theme.role(Role::TextMuted)),
        ]),
        width as u16,
    ))
}

/// Draws the header into `area` (one or two rows): logo, tabs, Back, crumbs and the account
/// chip, every control a key-bar style token. Hits keep their order: tabs, Source identity
/// (painted in the content pane's top border by [`main_pane`]), Back, then breadcrumbs.
pub(crate) fn draw_header(frame: &mut Frame, app: &mut App, area: Rect) {
    let theme = app.theme();
    let row0 = Rect::new(area.x, area.y, area.width, 1);
    let row1 = if area.height > 1 {
        Rect::new(area.x, area.y + 1, area.width, 1)
    } else {
        row0
    };
    let logo = Rect::new(row0.x, row0.y, (LOGO.len() as u16).min(row0.width), 1);
    frame.render_widget(
        Paragraph::new(LOGO).style(theme.filled(Hue::for_view(app.view))),
        logo,
    );
    let mut x = logo.right().saturating_add(1).min(row0.right());
    for (index, view) in View::ALL.into_iter().enumerate() {
        let (key, label) = tab_label(app, index, view);
        let text = format!(" {key} {label} ");
        let width =
            (crate::editor::display_width(&text) as u16).min(row0.right().saturating_sub(x));
        let rect = Rect::new(x, row0.y, width, 1);
        let view_theme = theme.with_view(view);
        // The active tab is filled with its view hue; the others show the key in that hue.
        let mut line = if app.view == view {
            Line::from(Span::styled(text, view_theme.filled(Hue::for_view(view))))
        } else {
            Line::from(vec![
                Span::styled(format!(" {key}"), view_theme.role(Role::Accent)),
                Span::styled(format!(" {label} "), theme.role(Role::TextMuted)),
            ])
        };
        if app.focus == index {
            line = line.patch_style(Style::default().add_modifier(Modifier::UNDERLINED));
        }
        frame.render_widget(Paragraph::new(line), rect);
        app.hits.push(HitRegion {
            rect,
            action: Action::View(view),
        });
        x = x.saturating_add(width).saturating_add(1).min(row0.right());
    }
    // Source identity is painted in the pane border just below; its hit keeps its place.
    let screen = frame.area();
    if area.bottom() < screen.bottom() {
        let pane = Rect::new(
            area.x,
            area.bottom(),
            area.width,
            screen.bottom() - area.bottom(),
        );
        // Beside a preview the token closes the list pane's border, so its hit follows it.
        let pane = if crate::list_preview_v92::offered(app)
            && splits()
            && pane.width >= crate::list_preview_v92::min_width(app)
        {
            split_list_preview(pane).list
        } else {
            pane
        };
        if let Some((rect, _)) = source_token(app, pane) {
            app.hits.push(HitRegion {
                rect,
                action: Action::RepositorySource,
            });
        }
    }
    let single = area.height == 1;
    let nav_row = if single { row0 } else { row1 };
    let nav_start = if single { x } else { nav_row.x };
    let before = app.hits.len();
    crate::navigation_state::draw_navigation(
        frame,
        app,
        Rect::new(
            nav_start,
            nav_row.y,
            nav_row.right().saturating_sub(nav_start),
            1,
        ),
    );
    // The account chip takes the free end of the first row; it is information, not a control.
    let used = if single {
        app.hits[before..]
            .iter()
            .map(|hit| hit.rect.right().saturating_add(1))
            .max()
            .unwrap_or(x)
            .max(x)
    } else {
        x
    };
    if let Some((line, width)) = account_line(app, theme) {
        if row0.right().saturating_sub(used) > width {
            frame.render_widget(
                Paragraph::new(line),
                Rect::new(row0.right() - width, row0.y, width, 1),
            );
        }
    }
}

/// A one-row strip of a view's Mode controls (sub-modes such as Runs/Workflows), styled like the
/// header tabs: the selected mode filled with the view hue, the others with the key in the hue.
/// Each tab is a click target pushed to `app.hits` in order; a disabled mode is dimmed and its
/// click explains why. Controls of other groups belong to the footer and are skipped. When the
/// padded full tabs do not fit the row, every tab uses its compact label without padding.
pub fn tab_strip(frame: &mut Frame, app: &mut App, area: Rect, controls: &[KeyControl]) {
    let theme = app.theme();
    if area.is_empty() {
        return;
    }
    let row = Rect::new(area.x, area.y, area.width, 1);
    frame.render_widget(Paragraph::new("").style(theme.base()), row);
    let modes = || {
        controls
            .iter()
            .filter(|control| control.group == ControlGroup::Mode)
    };
    // Padded full tabs with one-column gaps when they fit; otherwise compact labels without
    // padding, two columns apart like the key bar.
    let full = modes()
        .map(|control| crate::editor::display_width(&control.token(false)) + 3)
        .sum::<usize>()
        .saturating_sub(1);
    let compact = full > usize::from(row.width);
    let (pad, gap) = if compact { ("", 2) } else { (" ", 1) };
    let tab_text = |control: &KeyControl| {
        format!(
            "{pad}{} {}{pad}",
            safe_text(&control.key),
            safe_text(control.label_text(compact))
        )
    };
    let tab_width = |control: &KeyControl| {
        crate::editor::display_width(&tab_text(control)).min(usize::from(u16::MAX)) as u16
    };
    // How many tabs fit; when some do not, keep room for a dim `…` so the row does not
    // pretend to be complete (the rest stay one key or Commands entry away).
    let total = modes().count();
    let fitting = |reserve: u16| {
        let mut x = row.x;
        let mut count = 0;
        for control in modes() {
            let end = x.saturating_add(tab_width(control));
            if end > row.right() || (count + 1 < total && end.saturating_add(reserve) > row.right())
            {
                break;
            }
            count += 1;
            x = end.saturating_add(gap);
        }
        count
    };
    let mut shown = fitting(0);
    if shown < total {
        shown = fitting(gap + 1);
    }
    let mut x = row.x;
    for control in modes().take(shown) {
        let key = safe_text(&control.key);
        let label = safe_text(control.label_text(compact));
        let text = tab_text(control);
        let width = tab_width(control);
        let rect = Rect::new(x, row.y, width, 1);
        let mut line = if control.disabled.is_some() {
            Line::from(Span::styled(text, theme.role(Role::TextDim)))
        } else if control.selected {
            Line::from(Span::styled(text, theme.filled(Hue::for_view(app.view))))
        } else {
            Line::from(vec![
                Span::styled(format!("{pad}{key}"), theme.role(Role::Accent)),
                Span::styled(format!(" {label}{pad}"), theme.role(Role::TextMuted)),
            ])
        };
        if app.focus == app.hits.len() {
            line = line.patch_style(Style::default().add_modifier(Modifier::UNDERLINED));
        }
        frame.render_widget(Paragraph::new(line), rect);
        app.hits.push(HitRegion {
            rect,
            action: match &control.disabled {
                Some(reason) => Action::Explain(reason.clone()),
                None => control.action.clone(),
            },
        });
        x = x.saturating_add(width).saturating_add(gap);
    }
    if shown < total && x < row.right() {
        frame.render_widget(
            Paragraph::new(Span::styled("…", theme.role(Role::TextDim))),
            Rect::new(x, row.y, 1, 1),
        );
    }
}

/// Gives cells left at the terminal's default colors (by `Clear` or unstyled text) the theme's
/// base foreground and background. Without a palette, as in 16-color or NoColor modes, the
/// terminal defaults are the base, so nothing changes.
pub fn fill_default_colors(buffer: &mut Buffer, theme: Theme) {
    let base = theme.base();
    let (Some(fg), Some(bg)) = (base.fg, base.bg) else {
        return;
    };
    if bg == Color::Reset {
        return;
    }
    for cell in &mut buffer.content {
        if cell.bg == Color::Reset {
            cell.bg = bg;
        }
        if cell.fg == Color::Reset {
            cell.fg = fg;
        }
    }
}

/// From this terminal width a list and its preview sit side by side; narrower screens drill
/// down from the list into the detail instead (TASK-92).
pub const SPLIT_MIN_WIDTH: u16 = 120;

thread_local! {
    static DENSITY: std::cell::Cell<Density> = const { std::cell::Cell::new(Density::Comfortable) };
}

/// The density of the frame being drawn (TASK-96). [`crate::ui::draw`] sets it from the app
/// before any view paints, so layout helpers and [`crate::ui_table`] read one value per frame.
pub fn density() -> Density {
    DENSITY.with(std::cell::Cell::get)
}

/// Sets the density the next painted views read; see [`density`].
pub fn set_density(density: Density) {
    DENSITY.with(|cell| cell.set(density));
}

/// Whether a list may sit beside its preview at this density: Compact keeps every column for
/// the list and drills down with Enter at any width.
pub fn splits() -> bool {
    !density().is_compact()
}

/// The list pane and, when the screen is wide enough, the preview pane beside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListPreview {
    pub list: Rect,
    pub preview: Option<Rect>,
}

/// Splits a content area into a list pane (60%) and a preview pane at
/// [`SPLIT_MIN_WIDTH`] columns or more; below that, or at Compact density, the list keeps the
/// whole area.
pub fn split_list_preview(area: Rect) -> ListPreview {
    if area.width < SPLIT_MIN_WIDTH || area.height < 3 || !splits() {
        return ListPreview {
            list: area,
            preview: None,
        };
    }
    let list = area.width * 3 / 5;
    ListPreview {
        list: Rect::new(area.x, area.y, list, area.height),
        preview: Some(Rect::new(
            area.x + list,
            area.y,
            area.width - list,
            area.height,
        )),
    }
}

/// Whether the main view content, rather than header or footer chrome, owns focus.
pub fn content_focused(app: &App, chrome_hits: usize) -> bool {
    if app.overlay.is_some() {
        return false;
    }
    match app.view {
        View::Inbox => app.github_content_focused,
        View::Repository => app.repository_content_focused,
        View::Actions => app.actions_content_focused,
        View::Dashboard => app.metrics_content_focused,
        // Entering Search focuses its content like the other views; Tab moves focus to an
        // explicit control and the pane stays marked while a Search control holds focus.
        View::Search => app.search_content_focused || app.focus >= chrome_hits,
    }
}

/// The main view pane: `View • focused view` on the left (tests and screen readers rely on the
/// acknowledgement), the scope context on the right, and on Repository the `I Source identity`
/// token closing the top border (its hit is registered by [`draw_header`]).
pub fn main_pane(app: &App, theme: Theme, area: Rect, focused: bool) -> Block<'static> {
    let title = format!("{} • focused view", app.view.label());
    let left = crate::editor::display_width(&crumb_title(&[&title], focused)) as u16 + 1;
    let block = pane_block(theme, &[&title], focused);
    let budget = area.width.saturating_sub(2).saturating_sub(left);
    let Some((rect, label)) = source_token(app, area) else {
        return with_right_title(block, theme, focused, &app.context, budget);
    };
    let (border, _) = pane_styles(theme, focused);
    // The scope keeps the rest of the border: ` scope `, one border cell, then ` token ─`.
    let scope_width = budget.saturating_sub(rect.width + 4).saturating_sub(4);
    let block = if scope_width >= 8 && !app.context.is_empty() {
        let text = compact_scope(&safe_text(&app.context), scope_width);
        block.title(
            Line::from(Span::styled(
                format!(" {text} "),
                theme.role(Role::TextMuted),
            ))
            .right_aligned(),
        )
    } else {
        block
    };
    let token_focused = !app.repository_content_focused
        && app
            .hits
            .iter()
            .position(|hit| hit.action == Action::RepositorySource)
            .is_some_and(|index| index == app.focus);
    let mut spans = vec![Span::styled(" ", theme.role(Role::TextMuted))];
    spans.extend(key_token_line(theme, SOURCE_KEY, label, token_focused).spans);
    spans.extend([
        Span::styled(" ", theme.role(Role::TextMuted)),
        Span::styled("─", border),
    ]);
    block.title(Line::from(spans).right_aligned())
}

#[cfg(test)]
#[path = "ui_chrome_tests.rs"]
mod tests;
