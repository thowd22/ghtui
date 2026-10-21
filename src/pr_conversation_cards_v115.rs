//! PR Conversation as separated comment cards with visible actions (TASK-115).
//!
//! Every button here holds the exact action the PR actions menu (`p`) exposes for the same
//! command, so a click, its key and the menu share one workflow with the existing previews
//! and confirmations. Nothing here writes to GitHub or invents write semantics: Quote reply
//! opens the existing New comment composer and pastes the quoted text into its empty body,
//! exactly as a user paste would.
use crate::{
    app::{Action, App, HitRegion},
    footer::KeyControl,
    github_workspace::{PrCardV115, PrTab},
    providers::gh::{CommentKind, ConversationComment},
    theme::{Role, Theme},
};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

/// A per-comment button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardControlV115 {
    QuoteReply,
    React,
    Edit,
    Delete,
    /// The reactions row: browse who reacted (read only).
    Reactions,
}

/// A PR action bar button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarControlV115 {
    Comment,
    Approve,
    RequestChanges,
    Merge,
    Close,
    Ready,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CardsActionV115 {
    /// A card button: select the comment whose header is row `row`, then run the control.
    Card {
        row: usize,
        control: CardControlV115,
    },
    /// A key: run the control on the selected comment.
    Selected(CardControlV115),
    Bar(BarControlV115),
}

/// (control, key, label) of the per-comment buttons, in card order.
pub const CARD_CONTROLS: [(CardControlV115, &str, &str); 4] = [
    (CardControlV115::QuoteReply, "Q", "Quote reply"),
    (CardControlV115::React, "+", "React"),
    (CardControlV115::Edit, "E", "Edit"),
    (CardControlV115::Delete, "D", "Delete"),
];

/// (control, key, label) of the PR action bar, in bar order.
pub const BAR_CONTROLS: [(BarControlV115, &str, &str); 6] = [
    (BarControlV115::Comment, "n", "Comment"),
    (BarControlV115::Approve, "A", "Approve"),
    (BarControlV115::RequestChanges, "X", "Request changes"),
    (BarControlV115::Merge, "M", "Merge"),
    (BarControlV115::Close, "Z", "Close"),
    (BarControlV115::Ready, "Y", "Ready"),
];

/// The New comment entry of the PR actions menu, under either of its labels.
const NEW_COMMENT_LABELS: [&str; 2] = [
    "New comment • eligibility UNKNOWN/server-enforced",
    "New conversation comment",
];

/// A comment card header: `comment #ID • created UTC • age • badges • edited`. The id and
/// full UTC time stay visible; the badge is display only.
pub fn comment_header(comment: &ConversationComment, pr_author: &str) -> String {
    let mut parts = vec![
        format!("comment #{}", comment.id),
        comment.created_at.clone(),
    ];
    if let Some(age) = crate::ui_table::age_at(&comment.created_at, crate::storage::now()) {
        parts.push(format!("{age} ago"));
    }
    parts.extend(badges(
        &comment.author.login,
        comment.author_association(),
        pr_author,
    ));
    if !comment.updated_at.is_empty() && comment.updated_at != comment.created_at {
        parts.push("edited".into());
    }
    parts.join(" • ")
}

/// A review card header; it keeps the `review ID ` start Timeline links look for.
pub fn review_header(review: &serde_json::Value, pr_author: &str) -> String {
    let state = match review["state"].as_str().unwrap_or("unknown") {
        "APPROVED" => "approved",
        "CHANGES_REQUESTED" => "requested changes",
        "COMMENTED" => "reviewed",
        "DISMISSED" => "review dismissed",
        "PENDING" => "pending review (not submitted)",
        other => other,
    }
    .to_owned();
    let mut parts = vec![format!("review {}", review["id"]), state];
    let submitted = review["submitted_at"].as_str().unwrap_or_default();
    if !submitted.is_empty() {
        parts.push(submitted.to_owned());
        if let Some(age) = crate::ui_table::age_at(submitted, crate::storage::now()) {
            parts.push(format!("{age} ago"));
        }
    }
    parts.extend(badges(
        review["user"]["login"].as_str().unwrap_or_default(),
        review["author_association"].as_str().unwrap_or_default(),
        pr_author,
    ));
    parts.join(" • ")
}

/// Role badges: `Author` for the PR's author, then GitHub's association in words.
fn badges(login: &str, association: &str, pr_author: &str) -> Vec<String> {
    let mut badges = Vec::new();
    if !login.is_empty() && login.eq_ignore_ascii_case(pr_author) {
        badges.push("Author".to_owned());
    }
    let association = match association {
        "OWNER" => "Owner",
        "MEMBER" => "Member",
        "COLLABORATOR" => "Collaborator",
        "CONTRIBUTOR" => "Contributor",
        "FIRST_TIME_CONTRIBUTOR" | "FIRST_TIMER" => "First-time contributor",
        _ => "",
    };
    if !association.is_empty() {
        badges.push(association.to_owned());
    }
    badges
}

/// `👍 2  🎉 1` for nonzero reaction counts; None without reactions.
pub fn reaction_summary(reactions: &[(String, u64)]) -> Option<String> {
    let parts = reactions
        .iter()
        .filter(|(_, count)| *count > 0)
        .map(|(content, count)| {
            let glyph = match content.as_str() {
                "+1" => "👍",
                "-1" => "👎",
                "laugh" => "😄",
                "hooray" => "🎉",
                "confused" => "😕",
                "heart" => "❤",
                "rocket" => "🚀",
                "eyes" => "👀",
                other => other,
            };
            format!("{glyph} {count}")
        })
        .collect::<Vec<_>>();
    (!parts.is_empty()).then(|| parts.join("  "))
}

/// Whether the open PR page shows Conversation.
pub fn offered(app: &App) -> bool {
    app.github
        .as_ref()
        .and_then(|github| github.pr.as_ref())
        .is_some_and(|page| page.tab == PrTab::Conversation)
}

fn find(controls: &[(String, Action)], labels: &[&str]) -> Option<Action> {
    controls
        .iter()
        .find(|(label, _)| labels.contains(&label.as_str()))
        .map(|(_, action)| action.clone())
}

fn menu_labels(control: CardControlV115) -> &'static [&'static str] {
    match control {
        CardControlV115::QuoteReply => &NEW_COMMENT_LABELS,
        CardControlV115::React => &["React to selected comment"],
        CardControlV115::Edit => &["Edit selected comment"],
        CardControlV115::Delete => &["Delete selected comment"],
        CardControlV115::Reactions => &["Browse selected comment reactions"],
    }
}

/// The menu action of a card control for the comment selected at `row`.
pub fn card_action_at(app: &App, row: usize, control: CardControlV115) -> Action {
    let comment = app
        .github
        .as_ref()
        .and_then(|github| github.pr.as_ref())
        .and_then(|page| page.rows.get(row))
        .and_then(|row| row.comment.clone());
    let Some(comment) = comment else {
        return Action::Explain(
            "Select an exact comment header or body first, using arrows or a mouse click.".into(),
        );
    };
    if control == CardControlV115::QuoteReply && comment.kind != CommentKind::Conversation {
        return Action::Explain(
            "Quote reply answers a Conversation comment; reply to review comments in their thread."
                .into(),
        );
    }
    find(&app.pr_controls_at(row), menu_labels(control)).unwrap_or_else(|| {
        Action::Explain(format!(
            "{} is not available for this comment.",
            label(control)
        ))
    })
}

fn label(control: CardControlV115) -> &'static str {
    CARD_CONTROLS
        .iter()
        .find(|(candidate, _, _)| *candidate == control)
        .map_or("Reactions", |(_, _, label)| label)
}

/// The menu action of a PR action bar control (with its disabled explanation).
pub fn bar_action(app: &App, control: BarControlV115) -> Action {
    let controls = app.pr_controls();
    let labels: &[&str] = match control {
        BarControlV115::Comment => &NEW_COMMENT_LABELS,
        BarControlV115::Approve => &["Approve review"],
        BarControlV115::RequestChanges => &["Request changes"],
        BarControlV115::Merge => &["Merge pull request"],
        BarControlV115::Close => &["Close without merging"],
        BarControlV115::Ready => &["Mark ready for review"],
    };
    find(&controls, labels).unwrap_or_else(|| {
        let summary = app
            .github
            .as_ref()
            .and_then(|github| github.pr.as_ref())
            .map(|page| &page.details.summary);
        Action::Explain(match (control, summary) {
            (BarControlV115::Ready, Some(summary)) => crate::pr_actions::lifecycle_disabled_reason(
                crate::pr_actions::PrCommand::Ready,
                summary,
            )
            .unwrap_or_else(|| "This PR is already ready for review.".into()),
            _ => "Open an exact PR before choosing a PR action.".into(),
        })
    })
}

fn reason(action: &Action) -> Option<String> {
    match action {
        Action::Explain(reason) => Some(reason.clone()),
        _ => None,
    }
}

/// Conversation keys: the action bar and the selected comment's buttons. They leave a narrow
/// key bar first (the action bar and cards show them); every one keeps its key.
pub fn footer_keys(app: &App) -> Vec<KeyControl> {
    if !offered(app) || app.overlay.is_some() {
        return Vec::new();
    }
    let mut keys = BAR_CONTROLS
        .iter()
        .enumerate()
        .map(|(index, (control, key, text))| {
            let action = bar_action(app, *control);
            let reason = reason(&action);
            KeyControl::new(
                key.to_string(),
                *text,
                Action::PrCardsV115(CardsActionV115::Bar(*control)),
            )
            .with_priority(100 + index as u8)
            .disabled_by(reason)
        })
        .collect::<Vec<_>>();
    let selected = app.inbox_list.selected;
    keys.extend(
        CARD_CONTROLS
            .iter()
            .enumerate()
            .map(|(index, (control, key, text))| {
                let action = card_action_at(app, selected, *control);
                KeyControl::new(
                    key.to_string(),
                    *text,
                    Action::PrCardsV115(CardsActionV115::Selected(*control)),
                )
                .with_priority(110 + index as u8)
                .disabled_by(reason(&action))
            }),
    );
    keys
}

/// The header row of the comment shown at `row` (its card's list stop).
fn header_row(app: &App, row: usize) -> Option<usize> {
    let page = app.github.as_ref()?.pr.as_ref()?;
    let id = page.rows.get(row)?.comment.as_ref()?.observed.id;
    page.rows.iter().position(|candidate| {
        candidate.actor.is_some()
            && candidate
                .comment
                .as_ref()
                .is_some_and(|comment| comment.observed.id == id)
    })
}

/// The nearest comment row above card chrome row `row`: the comment the chrome belongs to.
fn owner_row(app: &App, row: usize) -> Option<usize> {
    let page = app.github.as_ref()?.pr.as_ref()?;
    (0..row.min(page.rows.len()))
        .rev()
        .take_while(|index| {
            !matches!(
                page.rows[*index].card,
                Some(PrCardV115::Top | PrCardV115::Buttons)
            )
        })
        .find(|index| page.rows[*index].comment.is_some())
}

/// Runs a card or bar control through the same action the PR actions menu holds.
pub fn dispatch(app: &mut App, action: CardsActionV115) {
    app.quote_reply_v115 = None;
    let (row, control) = match action {
        CardsActionV115::Bar(control) => {
            let action = bar_action(app, control);
            app.dispatch(action);
            return;
        }
        CardsActionV115::Selected(control) => (app.inbox_list.selected, control),
        CardsActionV115::Card { row, control } => {
            // The button's own comment becomes the selection, as a click on its header would.
            if let Some(header) = header_row(app, row) {
                app.inbox_list.selected = header;
                app.github_content_focused = true;
            }
            (app.inbox_list.selected, control)
        }
    };
    let action = card_action_at(app, row, control);
    if control == CardControlV115::QuoteReply && !matches!(action, Action::Explain(_)) {
        let body = app
            .github
            .as_ref()
            .and_then(|github| github.pr.as_ref())
            .and_then(|page| page.rows.get(row))
            .and_then(|row| row.comment.as_ref())
            .map(|comment| {
                (
                    comment.observed.author.login.clone(),
                    comment.observed.body.clone(),
                )
            });
        if let Some((author, body)) = body {
            app.quote_reply_v115 = Some(quote(&author, &body));
        }
    }
    app.dispatch(action);
}

/// GitHub's quote reply text: every line of the body behind `> `, then an empty line.
pub fn quote(author: &str, body: &str) -> String {
    let mut text = String::new();
    if !author.is_empty() {
        text.push_str(&format!("> @{author} wrote:\n"));
    }
    for line in body.lines() {
        if line.is_empty() {
            text.push_str(">\n");
        } else {
            text.push_str(&format!("> {line}\n"));
        }
    }
    text.push('\n');
    text
}

/// After a form opens: a pending quote reply is pasted into an empty New comment body,
/// exactly as a user paste. A restored draft or any other form keeps its text.
pub fn apply_quote(app: &mut App) {
    let Some(quote) = app.quote_reply_v115.take() else {
        return;
    };
    let Some(form) = app.form.as_mut() else {
        return;
    };
    let empty_body = form
        .fields
        .get(form.focus)
        .is_some_and(|field| field.key == "body" && field.editor.text.is_empty());
    if empty_body && !form.restored_draft && !form.preview && !form.submitting {
        form.paste(&quote);
    }
}

/// Paints the PR action bar into `area` (one row) and returns its hits.
pub fn draw_bar(buffer: &mut Buffer, app: &App, area: Rect, theme: Theme) -> Vec<HitRegion> {
    let mut hits = Vec::new();
    let mut x = area.x.saturating_add(2);
    for (control, key, text) in BAR_CONTROLS {
        let action = bar_action(app, control);
        let disabled = matches!(action, Action::Explain(_));
        let width = token(buffer, x, area, theme, key, text, disabled);
        if width == 0 {
            break;
        }
        hits.push(HitRegion {
            rect: Rect::new(x, area.y, width, 1),
            action: if disabled {
                action
            } else {
                Action::PrCardsV115(CardsActionV115::Bar(control))
            },
        });
        x = x.saturating_add(width + 1);
    }
    hits
}

/// Paints the buttons of the comment card whose row is `row` and returns their hits.
pub fn draw_buttons(
    buffer: &mut Buffer,
    app: &App,
    area: Rect,
    theme: Theme,
    row: usize,
) -> Vec<HitRegion> {
    let mut hits = Vec::new();
    let mut x = area.x.saturating_add(2);
    for (control, key, text) in CARD_CONTROLS {
        let action = card_action_at(app, row, control);
        let disabled = matches!(action, Action::Explain(_));
        let width = token(buffer, x, area, theme, key, text, disabled);
        if width == 0 {
            break;
        }
        hits.push(HitRegion {
            rect: Rect::new(x, area.y, width, 1),
            action: if disabled {
                action
            } else {
                Action::PrCardsV115(CardsActionV115::Card { row, control })
            },
        });
        x = x.saturating_add(width + 1);
    }
    hits
}

/// `[key Label]` as a button; a disabled one is dim (its click explains why). Returns the
/// painted width, or 0 when it does not fit.
fn token(
    buffer: &mut Buffer,
    x: u16,
    area: Rect,
    theme: Theme,
    key: &str,
    text: &str,
    disabled: bool,
) -> u16 {
    let width = (crate::editor::display_width(key) + crate::editor::display_width(text) + 3) as u16;
    if x.saturating_add(width) > area.right() {
        return 0;
    }
    let (bracket, key_style, label_style) = if disabled {
        (
            theme.role(Role::TextDim),
            theme.role(Role::TextDim),
            theme.role(Role::TextDim),
        )
    } else {
        (
            theme.role(Role::Border),
            theme
                .role(Role::Accent)
                .add_modifier(ratatui::style::Modifier::BOLD),
            theme.role(Role::Text),
        )
    };
    Paragraph::new(Line::from(vec![
        Span::styled("[", bracket),
        Span::styled(key.to_owned(), key_style),
        Span::styled(" ", label_style),
        Span::styled(text.to_owned(), label_style),
        Span::styled("]", bracket),
    ]))
    .render(Rect::new(x, area.y, width, 1), buffer);
    width
}

/// The persistent `Add a comment` box at the bottom of Conversation (one row).
pub fn draw_add_comment(
    buffer: &mut Buffer,
    app: &App,
    area: Rect,
    theme: Theme,
) -> Vec<HitRegion> {
    let action = bar_action(app, BarControlV115::Comment);
    let disabled = matches!(action, Action::Explain(_));
    let border = theme.role(Role::Border);
    let inner = usize::from(area.width.saturating_sub(4));
    let hint = if disabled { "" } else { "n" };
    let text = "✎ Add a comment…";
    let used = crate::editor::display_width(text) + crate::editor::display_width(hint) + 2;
    let gap = inner.saturating_sub(used);
    let line = Line::from(vec![
        Span::styled("  [ ", border),
        Span::styled(
            text,
            if disabled {
                theme.role(Role::TextDim)
            } else {
                theme.role(Role::TextMuted)
            },
        ),
        Span::styled(" ".repeat(gap), Style::default()),
        Span::styled(
            hint,
            theme
                .role(Role::Accent)
                .add_modifier(ratatui::style::Modifier::BOLD),
        ),
        Span::styled(" ]", border),
    ]);
    Paragraph::new(line).render(area, buffer);
    vec![HitRegion {
        rect: area,
        action: if disabled {
            action
        } else {
            Action::PrCardsV115(CardsActionV115::Bar(BarControlV115::Comment))
        },
    }]
}

/// Paints a card chrome row (opening rule, reactions line or buttons); returns its hits.
pub fn draw_chrome(
    buffer: &mut Buffer,
    app: &App,
    area: Rect,
    theme: Theme,
    card: &PrCardV115,
    row: usize,
    label: &str,
) -> Vec<HitRegion> {
    // Reactions and buttons act on the comment whose rows end just above them.
    let row = owner_row(app, row).unwrap_or(row);
    match card {
        PrCardV115::Top => {
            let width = usize::from(area.width.saturating_sub(2));
            Paragraph::new(Span::styled(
                format!("  {}", "─".repeat(width)),
                theme.role(Role::Border),
            ))
            .render(area, buffer);
            Vec::new()
        }
        PrCardV115::Reactions => {
            Paragraph::new(Span::styled(
                format!("  {}", crate::app::safe_text(label)),
                theme.role(Role::TextMuted),
            ))
            .render(area, buffer);
            let action = card_action_at(app, row, CardControlV115::Reactions);
            vec![HitRegion {
                rect: area,
                action: if matches!(action, Action::Explain(_)) {
                    action
                } else {
                    Action::PrCardsV115(CardsActionV115::Card {
                        row,
                        control: CardControlV115::Reactions,
                    })
                },
            }]
        }
        PrCardV115::Buttons => draw_buttons(buffer, app, area, theme, row),
        PrCardV115::Markdown(_) => Vec::new(),
    }
}

#[cfg(test)]
#[path = "pr_conversation_cards_v115_tests.rs"]
mod tests;
