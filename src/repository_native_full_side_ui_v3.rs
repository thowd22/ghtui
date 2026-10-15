//! Borrowed full-file widget. No provider work, matcher, body clone or authority.
use crate::{
    editor::{display_line, display_width},
    repository_native_diff_content_v2::NativeDiffPurposeV2,
    repository_native_diff_ui_v1::{
        paint_style, NativeDiffActionV1 as A, NativeDiffHitV1, NativeDiffLayoutV1,
    },
    repository_native_diff_v1::NativeDiffStateV1,
    repository_view::DiffSide,
    theme::{SemanticToken, Theme},
};
use ratatui::{
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

pub(crate) fn draw<S: NativeDiffPurposeV2>(
    layout: &mut NativeDiffLayoutV1<S>,
    frame: &mut Frame,
    state: &mut NativeDiffStateV1<S>,
    inner: Rect,
    theme: Theme,
) {
    if inner.width < 38 || inner.height < 16 {
        frame.render_widget(
            Paragraph::new(
                "Resize full-file context to40x18 or larger; original source/patch range retained",
            )
            .style(theme.style(SemanticToken::Muted)),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
        layout.button(
            frame,
            Rect::new(inner.x, inner.bottom() - 1, 11.min(inner.width), 1),
            "[Esc Close]",
            A::Close,
            theme,
        );
        if inner.height > 2 {
            layout.button(
                frame,
                Rect::new(inner.x, inner.bottom() - 2, 13.min(inner.width), 1),
                "[b Back patch]",
                A::BackToPatch,
                theme,
            );
        }
        return;
    }
    let mut y = inner.y;
    let current = state.current_full_side().is_some();
    let side = state.full_side();
    let title = side
        .map(|side| {
            if !current {
                return format!("Previous FULL {:?} file • NOT CURRENT", side.side());
            }
            format!(
                "FULL {:?} file • read-only context • patch range {} retained",
                side.side(),
                state.range().map_or(0, |range| range.changed().len())
            )
        })
        .unwrap_or_else(|| "Preparing FULL file • patch selection retained".into());
    frame.render_widget(
        Paragraph::new(title).style(theme.style(SemanticToken::Reference)),
        Rect::new(inner.x, y, inner.width, 1),
    );
    y += 1;
    if let Some(side) = side {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Path: ", theme.style(SemanticToken::Changed)),
                Span::styled(side.path_label(), theme.style(SemanticToken::Changed)),
            ])),
            Rect::new(inner.x, y, inner.width, 1),
        );
        y += 1;
        frame.render_widget(
            Paragraph::new(side.label()).style(theme.style(SemanticToken::Reference)),
            Rect::new(inner.x, y, inner.width, 1),
        );
        y += 1;
        for (identity, label) in [side.author(), side.committer()] {
            frame.render_widget(
                crate::identity::AvatarWidget {
                    identity,
                    image: None,
                    theme,
                },
                Rect::new(inner.x, y, 4.min(inner.width), 1),
            );
            frame.render_widget(
                Paragraph::new(label).style(theme.style(SemanticToken::Actor)),
                Rect::new(inner.x + 5, y, inner.width.saturating_sub(5), 1),
            );
            y += 1;
        }
        frame.render_widget(
            Paragraph::new(format!(
                "{} matching lines{} • plain {}",
                side.matches().len(),
                if side.matches_capped() {
                    " (10k occurrence cap)"
                } else {
                    ""
                },
                side.plain()
            ))
            .style(theme.style(SemanticToken::Muted)),
            Rect::new(inner.x, y, inner.width, 1),
        );
        y += 1;
        frame.render_widget(
            Paragraph::new(if current {
                side.coverage()
            } else {
                "RETAINED metadata • NOT CURRENT • no row authority"
            })
            .style(theme.style(SemanticToken::Reference)),
            Rect::new(inner.x, y, inner.width, 1),
        );
        y += 1;
        frame.render_widget(
            Paragraph::new(side.syntax()).style(theme.style(SemanticToken::Muted)),
            Rect::new(inner.x, y, inner.width, 1),
        );
        y += 1;
    }
    let controls = [
        ("[b Back patch]", A::BackToPatch),
        ("[E External]", A::External),
        ("[F Full Old]", A::FullSide(DiffSide::Old)),
        ("[V Full New]", A::FullSide(DiffSide::New)),
        ("[o Options]", A::Controls),
        ("[I Source]", A::Source),
        ("[n Match]", A::Match(1)),
        ("[Esc Close]", A::Close),
    ];
    let mut x = inner.x;
    for (label, action) in controls
        .into_iter()
        .chain(S::supports_inline_comment().then_some(("[P Comment disabled]", A::Comment)))
    {
        let width = label.len() as u16;
        if width > inner.width {
            continue;
        }
        if x + width > inner.right() {
            x = inner.x;
            y += 1;
        }
        if y >= inner.bottom().saturating_sub(4) {
            break;
        }
        layout.button(frame, Rect::new(x, y, width, 1), label, action, theme);
        x += width + 1;
    }
    y += 1;
    layout.button(
        frame,
        Rect::new(inner.x, y, 7, 1),
        if state.regex { "Regex:" } else { "Exact:" },
        A::FocusQuery,
        theme,
    );
    layout.query = Rect::new(inner.x + 7, y, inner.width.saturating_sub(7), 1);
    state
        .query
        .ensure_visible(usize::from(layout.query.width), 1);
    frame.render_widget(
        Paragraph::new(display_line(
            &state.query.text,
            state.query.left_column,
            usize::from(layout.query.width),
        ))
        .style(theme.base()),
        layout.query,
    );
    if layout.query_focused && !layout.query.is_empty() {
        let caret = display_width(&state.query.text[..state.query.cursor])
            .saturating_sub(state.query.left_column)
            .min(usize::from(layout.query.width.saturating_sub(1)));
        frame.set_cursor_position((layout.query.x + caret as u16, layout.query.y));
    }
    y += 1;
    layout.body = Rect::new(
        inner.x,
        y,
        inner.width,
        inner.bottom().saturating_sub(y + 3),
    );
    frame.render_widget(Paragraph::new("Full-file rows have NO patch/hunk/staging/inline coordinates; Back restores original selection").style(theme.style(SemanticToken::Muted)),Rect::new(inner.x,inner.bottom()-3,inner.width,1));
    frame.render_widget(
        Paragraph::new(state.status.as_str()).style(theme.style(SemanticToken::Reference)),
        Rect::new(inner.x, inner.bottom() - 2, inner.width, 1),
    );
    let mut footer_x = inner.x;
    for (label, action) in [
        ("[b Back patch]", A::BackToPatch),
        ("[o Options]", A::Controls),
        ("[Esc Close]", A::Close),
    ] {
        let width = label.len() as u16;
        if footer_x + width > inner.right() {
            break;
        }
        layout.button(
            frame,
            Rect::new(footer_x, inner.bottom() - 1, width, 1),
            label,
            action,
            theme,
        );
        footer_x += width + 1;
    }
    if !current {
        frame.render_widget(
            Paragraph::new(state.metadata_note().unwrap_or(
                "Preparing current full-file context • previous rows are NOT CURRENT; no row hits",
            ))
            .style(theme.style(SemanticToken::Muted)),
            layout.body,
        );
        return;
    }
    let Some(side) = state.current_full_side() else {
        return;
    };
    for (index, row) in side
        .rows()
        .iter()
        .enumerate()
        .skip(state.scroll)
        .take(usize::from(layout.body.height))
    {
        let rect = Rect::new(
            inner.x,
            layout.body.y + (index - state.scroll) as u16,
            inner.width,
            1,
        );
        let base = if index == state.selected {
            theme
                .style(SemanticToken::Normal)
                .add_modifier(Modifier::UNDERLINED)
        } else {
            theme.base()
        };
        let number = row
            .line()
            .map_or_else(|| "     ".into(), |line| format!("{line:>5}"));
        let mut spans = vec![Span::styled(
            format!(
                "{} {number}{} ",
                if index == state.selected { ">" } else { " " },
                if row.continuation() { "↳" } else { " " }
            ),
            base,
        )];
        let mut start = 0;
        for span in row.spans() {
            if span.bytes.start > start {
                spans.push(Span::styled(&row.text()[start..span.bytes.start], base));
            }
            spans.push(Span::styled(
                &row.text()[span.bytes.clone()],
                paint_style(span.role, base, theme),
            ));
            start = span.bytes.end;
        }
        if start < row.text().len() {
            spans.push(Span::styled(&row.text()[start..], base));
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans))
                .scroll((0, state.horizontal))
                .style(base),
            rect,
        );
        if let Some(ticket) = side.ticket(index) {
            layout.hits.push(NativeDiffHitV1 {
                rect,
                action: A::FullPoint { ticket, row: index },
            });
        }
    }
}
pub(crate) fn draw_controls<S: NativeDiffPurposeV2>(
    layout: &mut NativeDiffLayoutV1<S>,
    frame: &mut Frame,
    state: &NativeDiffStateV1<S>,
    inner: Rect,
    theme: Theme,
) {
    if inner.width < 38 || inner.height < 16 {
        frame.render_widget(
            Paragraph::new(
                "Resize full-file options to40x18 or larger; read-only source/range retained",
            )
            .style(theme.style(SemanticToken::Muted)),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
        layout.button(
            frame,
            Rect::new(inner.x, inner.bottom() - 1, 11.min(inner.width), 1),
            "[Esc Close]",
            A::Close,
            theme,
        );
        if inner.height > 2 {
            layout.button(
                frame,
                Rect::new(inner.x, inner.bottom() - 2, 13.min(inner.width), 1),
                "[b Back patch]",
                A::BackToPatch,
                theme,
            );
        }
        return;
    }
    use crate::repository_native_diff_v1::NativeDiffOptionChangeV1 as O;
    let entries = [
        ("[o Back to file]", A::Controls),
        ("[E External]", A::External),
        ("[b Back patch]", A::BackToPatch),
        ("[F Full Old]", A::FullSide(DiffSide::Old)),
        ("[V Full New]", A::FullSide(DiffSide::New)),
        ("[w Wrap]", A::Option(O::Wrap)),
        ("[s Whitespace]", A::Option(O::Whitespace)),
        ("[y Syntax]", A::Option(O::Syntax)),
        ("[r Exact / regex]", A::Regex),
        ("[/ Search full file]", A::FocusQuery),
        ("[p Previous match]", A::Match(-1)),
        ("[n Matching line]", A::Match(1)),
        (
            if inner.width >= 18 {
                "[a Stage disabled]"
            } else {
                "[a Disabled]"
            },
            A::Stage,
        ),
        ("[I Source]", A::Source),
        ("[Esc Close]", A::Close),
        ("[q Quit]", A::Quit),
    ];
    let mut x = inner.x;
    let mut y = inner.y;
    for (label, action) in entries {
        let width = (label.len() as u16).min(inner.width);
        if x + width > inner.right() {
            x = inner.x;
            y += 1;
        }
        if y >= inner.bottom() - 1 {
            break;
        }
        layout.button(frame, Rect::new(x, y, width, 1), label, action, theme);
        x += width + 1;
    }
    if y + 2 < inner.bottom() - 1 {
        frame.render_widget(Paragraph::new("Read-only full-file context. Search covers only the admitted full-file prefix. Wrap, whitespace marks and bounded syntax change presentation only. Long lines use plain fallback and at most4096 columns of horizontal navigation. Patch hunks/ranges/staging, split pairing and generated-line hiding require b Back patch.").wrap(ratatui::widgets::Wrap{trim:false}).style(theme.style(SemanticToken::Muted)),Rect::new(inner.x,y+2,inner.width,inner.bottom()-y-3));
    }
    frame.render_widget(
        Paragraph::new(state.status.as_str()).style(theme.style(SemanticToken::Reference)),
        Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
    );
}
#[cfg(test)]
#[path = "repository_native_full_side_ui_v3_tests.rs"]
mod tests;
