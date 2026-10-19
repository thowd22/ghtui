//! Shared chrome: the merged header row, rounded pane frames with breadcrumb titles, section
//! headers and label/value rows. Focus never relies on color alone: a focused pane also gets a
//! bold title with the `▶` marker, and the keyboard-focused header control stays underlined.
use crate::app::{safe_text, Action, App, HitRegion, View};
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
const SOURCE_CONTROL: &str = "[Source identity]";

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
    let budget = cells.saturating_sub(crate::editor::display_width(&lead));
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

/// Header rows: one when the logo, tabs, Back, crumbs and Source identity fit, else two, with
/// Back, the crumbs and Source identity on the second row.
pub fn header_rows(app: &App, width: u16) -> u16 {
    let source = if source_control(app) {
        SOURCE_CONTROL.len() as u16 + 1
    } else {
        0
    };
    let needed = tabs_width(app)
        .saturating_add(crate::navigation_state::navigation_width(app))
        .saturating_add(source);
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

/// Draws the header into `area` (one or two rows). Hits keep their order: tabs, Source
/// identity, Back, then breadcrumbs.
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
    let single = area.height == 1;
    let nav_row = if single { row0 } else { row1 };
    let nav_start = if single { x } else { nav_row.x };
    let mut nav_right = nav_row.right();
    if source_control(app) && nav_row.width > SOURCE_CONTROL.len() as u16 {
        let width = SOURCE_CONTROL.len() as u16;
        let rect = Rect::new(nav_row.right() - width, nav_row.y, width, 1);
        let focused = app.focus == app.hits.len() && !app.repository_content_focused;
        let style = if focused {
            theme
                .role(Role::Accent)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            theme.style(crate::theme::SemanticToken::Reference)
        };
        frame.render_widget(Paragraph::new(SOURCE_CONTROL).style(style), rect);
        app.hits.push(HitRegion {
            rect,
            action: Action::RepositorySource,
        });
        nav_right = rect.x.saturating_sub(1);
    }
    let before = app.hits.len();
    crate::navigation_state::draw_navigation(
        frame,
        app,
        Rect::new(nav_start, nav_row.y, nav_right.saturating_sub(nav_start), 1),
    );
    // The account chip takes the free end of the first row; it is information, not a control.
    let used = if single {
        app.hits[before..]
            .iter()
            .map(|hit| hit.rect.right())
            .max()
            .unwrap_or(x)
            .max(x)
    } else {
        x
    };
    let right = if single { nav_right } else { row0.right() };
    if let Some((line, width)) = account_line(app, theme) {
        if right.saturating_sub(used) > width {
            frame.render_widget(
                Paragraph::new(line),
                Rect::new(right - width, row0.y, width, 1),
            );
        }
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
/// acknowledgement), the scope context on the right.
pub fn main_pane(app: &App, theme: Theme, area: Rect, focused: bool) -> Block<'static> {
    let title = format!("{} • focused view", app.view.label());
    let left = crate::editor::display_width(&crumb_title(&[&title], focused)) as u16 + 1;
    let block = pane_block(theme, &[&title], focused);
    with_right_title(
        block,
        theme,
        focused,
        &app.context,
        area.width.saturating_sub(2).saturating_sub(left),
    )
}

#[cfg(test)]
#[path = "ui_chrome_tests.rs"]
mod tests;
