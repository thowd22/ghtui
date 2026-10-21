//! One GitHub-flavoured Markdown renderer for PR descriptions, conversation comments, review
//! bodies and previews (TASK-114).
//!
//! The renderer is display-only: it never changes the stored body, and every place that
//! publishes or edits a body keeps the raw text. HTML comments are hidden, common inline HTML
//! (`a`, `img`/`picture`, `details`/`summary`, `br`, emphasis tags) is reduced to readable
//! text, links keep their text and list their URL once under the body, and images read as
//! `[image: alt]`. Styles come from theme roles only.
use crate::theme::{Role, Theme};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use regex::Regex;
use std::sync::OnceLock;

/// The block a rendered line belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    Heading(u8),
    ListItem,
    Code,
    Rule,
    TableHeader,
    TableRule,
    TableRow,
    Blank,
    /// The `Links` list under a body that holds each link's URL.
    Links,
}

/// Inline emphasis of one span.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Inline {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub link: bool,
    pub image: bool,
    /// Decoration (bullets, gutters, link numbers): never the sole carrier of information.
    pub marker: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MdSpan {
    pub text: String,
    pub style: Inline,
}

/// One logical rendered line: a first-row prefix (bullet, gutter), the prefix continuation
/// rows hang under, and the content spans.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MdLine {
    pub kind: BlockKind,
    /// Block-quote depth: quoted lines read muted.
    pub quote: u8,
    pub prefix: Vec<MdSpan>,
    pub hang: Vec<MdSpan>,
    pub spans: Vec<MdSpan>,
}

impl MdLine {
    fn new(kind: BlockKind, spans: Vec<MdSpan>) -> Self {
        Self {
            kind,
            quote: 0,
            prefix: Vec::new(),
            hang: Vec::new(),
            spans,
        }
    }
    /// The line's visible text (prefix and content), as painted on one unwrapped row.
    pub fn plain(&self) -> String {
        self.prefix
            .iter()
            .chain(&self.spans)
            .map(|span| span.text.as_str())
            .collect()
    }
    /// The content without its prefix.
    pub fn content(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }
    /// Rows of at most `width` cells: the first starts with the prefix, the rest with the
    /// hanging indent. Words wrap at whitespace; overlong words hard-wrap. A rule fills the
    /// width. At least one row is returned.
    pub fn wrap(&self, width: usize) -> Vec<Vec<MdSpan>> {
        let width = width.max(1);
        if self.kind == BlockKind::Rule {
            let lead = spans_width(&self.prefix);
            let mut row = self.prefix.clone();
            row.push(MdSpan {
                text: "─".repeat(width.saturating_sub(lead).max(1)),
                style: Inline {
                    marker: true,
                    ..Inline::default()
                },
            });
            return vec![row];
        }
        let lead = spans_width(&self.prefix).max(spans_width(&self.hang));
        let room = width.saturating_sub(lead).max(width.min(8)).max(1);
        let text = self.content();
        let pieces = crate::github_ui::wrap_cells(&text, room);
        let mut rows = Vec::with_capacity(pieces.len());
        let mut at = 0usize;
        for (index, piece) in pieces.iter().enumerate() {
            let mut row = if index == 0 {
                self.prefix.clone()
            } else {
                self.hang.clone()
            };
            row.extend(slice_spans(&self.spans, at, at + piece.len()));
            at += piece.len();
            rows.push(row);
        }
        rows
    }
}

fn spans_width(spans: &[MdSpan]) -> usize {
    spans
        .iter()
        .map(|span| crate::editor::display_width(&span.text))
        .sum()
}

/// The parts of `spans` between byte offsets `start..end` of their joined text.
fn slice_spans(spans: &[MdSpan], start: usize, end: usize) -> Vec<MdSpan> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    for span in spans {
        let span_end = offset + span.text.len();
        let from = start.max(offset);
        let to = end.min(span_end);
        if from < to {
            out.push(MdSpan {
                text: span.text[from - offset..to - offset].to_owned(),
                style: span.style,
            });
        }
        offset = span_end;
        if offset >= end {
            break;
        }
    }
    out
}

/// A rendered body: its lines, then the numbered URL list when links were found.
pub fn render(body: &str) -> Vec<MdLine> {
    let mut links = Vec::new();
    let mut lines = Vec::new();
    render_into(body, &mut links, &mut lines);
    // No leading or trailing blank rows; one blank between blocks.
    while lines
        .last()
        .is_some_and(|line| line.kind == BlockKind::Blank)
    {
        lines.pop();
    }
    if !links.is_empty() {
        if !lines.is_empty() {
            lines.push(MdLine::new(BlockKind::Blank, Vec::new()));
        }
        lines.push(MdLine::new(
            BlockKind::Links,
            vec![MdSpan {
                text: "Links".into(),
                style: Inline {
                    bold: true,
                    marker: true,
                    ..Inline::default()
                },
            }],
        ));
        for (index, url) in links.iter().enumerate() {
            let mut line = MdLine::new(
                BlockKind::Links,
                vec![MdSpan {
                    text: crate::app::safe_text(url),
                    style: Inline {
                        link: true,
                        ..Inline::default()
                    },
                }],
            );
            let number = format!("[{}] ", index + 1);
            line.hang = vec![marker(&" ".repeat(number.len()))];
            line.prefix = vec![marker(&number)];
            lines.push(line);
        }
    }
    lines
}

/// The visible text of a rendered body, one string per logical line (tests, palette, logs).
pub fn plain_lines(body: &str) -> Vec<String> {
    render(body).iter().map(MdLine::plain).collect()
}

fn marker(text: &str) -> MdSpan {
    MdSpan {
        text: text.to_owned(),
        style: Inline {
            marker: true,
            ..Inline::default()
        },
    }
}

fn plain_span(text: &str) -> MdSpan {
    MdSpan {
        text: text.to_owned(),
        style: Inline::default(),
    }
}

/// The theme style of a span on a line of `kind`.
pub fn style(theme: Theme, line: &MdLine, inline: Inline) -> Style {
    let mut style = if inline.marker {
        theme.role(Role::TextMuted)
    } else if inline.code || line.kind == BlockKind::Code {
        theme.role(Role::Info)
    } else if inline.link {
        theme.role(Role::Info).add_modifier(Modifier::UNDERLINED)
    } else if inline.image {
        theme.role(Role::TextMuted)
    } else {
        match line.kind {
            BlockKind::Heading(level) if level <= 2 => {
                theme.role(Role::Accent).add_modifier(Modifier::BOLD)
            }
            BlockKind::Heading(_) => theme.role(Role::Text).add_modifier(Modifier::BOLD),
            BlockKind::TableHeader => theme.role(Role::Text).add_modifier(Modifier::BOLD),
            BlockKind::TableRule | BlockKind::Rule => theme.role(Role::TextDim),
            _ if line.quote > 0 => theme.role(Role::TextMuted),
            _ => theme.role(Role::Text),
        }
    };
    if inline.bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    if inline.italic {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if inline.strike {
        style = style.add_modifier(Modifier::CROSSED_OUT);
    }
    style
}

/// Ratatui lines for a body at `width` cells (prefixes hang on wrapped rows).
pub fn lines(theme: Theme, body: &str, width: usize) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    for line in render(body) {
        for mut row in line.wrap(width) {
            // A wrapped row's trailing space may pass the edge; it is never painted.
            if let Some(last) = row.last_mut().filter(|_| line.kind != BlockKind::Code) {
                let kept = last.text.trim_end().len();
                last.text.truncate(kept);
            }
            out.push(Line::from(
                row.into_iter()
                    .map(|span| Span::styled(span.text, style(theme, &line, span.style)))
                    .collect::<Vec<_>>(),
            ));
        }
    }
    out
}

fn regex(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("valid Markdown pattern"))
}

/// Fenced code lines stay verbatim; everything else has its HTML reduced, then is parsed as
/// blocks. Quotes recurse through here.
fn render_into(text: &str, links: &mut Vec<String>, out: &mut Vec<MdLine>) {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut prose = String::new();
    let mut fence: Option<(char, usize)> = None;
    let mut code: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let trimmed = raw.trim_start();
        let indent = raw.len() - trimmed.len();
        let run = |c: char| trimmed.chars().take_while(|x| *x == c).count();
        match fence {
            Some((c, n)) => {
                if indent < 4 && run(c) >= n && trimmed[run(c) * c.len_utf8()..].trim().is_empty() {
                    flush_code(&mut code, out);
                    fence = None;
                } else {
                    code.push(raw.to_owned());
                }
            }
            None => {
                let opener = ['`', '~']
                    .into_iter()
                    .find(|c| indent < 4 && run(*c) >= 3)
                    .filter(|c| *c == '~' || !trimmed[run(*c)..].contains('`'));
                if let Some(c) = opener {
                    blocks(&reduce_html(&prose), links, out);
                    prose.clear();
                    fence = Some((c, run(c)));
                    // An info string (language) heads the block as a dim label.
                    let info = trimmed[run(c) * c.len_utf8()..].trim();
                    if !info.is_empty() {
                        let mut label = MdLine::new(
                            BlockKind::Code,
                            vec![marker(&crate::app::safe_text(
                                info.split_whitespace().next().unwrap_or(info),
                            ))],
                        );
                        label.prefix = vec![marker("┃ ")];
                        label.hang = vec![marker("┃ ")];
                        out.push(label);
                    }
                } else {
                    prose.push_str(raw);
                    prose.push('\n');
                }
            }
        }
    }
    if fence.is_some() {
        flush_code(&mut code, out);
    }
    blocks(&reduce_html(&prose), links, out);
}

fn flush_code(code: &mut Vec<String>, out: &mut Vec<MdLine>) {
    for raw in code.drain(..) {
        let mut line = MdLine::new(
            BlockKind::Code,
            vec![MdSpan {
                text: crate::app::safe_text(&raw.replace('\t', "    ")),
                style: Inline {
                    code: true,
                    ..Inline::default()
                },
            }],
        );
        line.prefix = vec![marker("┃ ")];
        line.hang = vec![marker("┃ ")];
        out.push(line);
    }
}

/// HTML a PR body commonly carries, reduced to Markdown-ish text: comments go, links and
/// images become Markdown links/images, block tags become line breaks, others disappear.
/// Inline code spans are left untouched.
pub fn reduce_html(text: &str) -> String {
    static CODE: OnceLock<Regex> = OnceLock::new();
    static COMMENT: OnceLock<Regex> = OnceLock::new();
    static OPEN_COMMENT: OnceLock<Regex> = OnceLock::new();
    static IMG: OnceLock<Regex> = OnceLock::new();
    static ANCHOR: OnceLock<Regex> = OnceLock::new();
    static SUMMARY: OnceLock<Regex> = OnceLock::new();
    static HEADING: OnceLock<Regex> = OnceLock::new();
    static BREAK: OnceLock<Regex> = OnceLock::new();
    static RULE: OnceLock<Regex> = OnceLock::new();
    static ITEM: OnceLock<Regex> = OnceLock::new();
    static BLOCK: OnceLock<Regex> = OnceLock::new();
    static STRONG: OnceLock<Regex> = OnceLock::new();
    static EMPHASIS: OnceLock<Regex> = OnceLock::new();
    static STRIKE: OnceLock<Regex> = OnceLock::new();
    static CODE_TAG: OnceLock<Regex> = OnceLock::new();
    static TAG: OnceLock<Regex> = OnceLock::new();
    // Hide inline code spans from the tag rewrites, then put them back.
    let code = regex(&CODE, r"`+[^`\n]*`+");
    let mut kept = Vec::new();
    let text = code.replace_all(text, |caps: &regex::Captures| {
        kept.push(caps[0].to_owned());
        format!("\u{1}{}\u{2}", kept.len() - 1)
    });
    let text = regex(&COMMENT, r"(?s)<!--.*?-->").replace_all(&text, "");
    let text = regex(&OPEN_COMMENT, r"(?s)<!--.*\z").replace_all(&text, "");
    let attr = |tag: &str, name: &str| -> Option<String> {
        let pattern = format!(r#"(?i)\b{name}\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#);
        let found = Regex::new(&pattern).ok()?.captures(tag)?;
        found
            .get(1)
            .or_else(|| found.get(2))
            .or_else(|| found.get(3))
            .map(|value| value.as_str().to_owned())
    };
    let text = regex(&IMG, r"(?is)<img\b[^>]*>").replace_all(&text, |caps: &regex::Captures| {
        let tag = &caps[0];
        let alt = attr(tag, "alt").unwrap_or_default();
        let src = attr(tag, "src").unwrap_or_default();
        format!("![{}]({})", alt.replace(['[', ']'], ""), src)
    });
    let text = regex(&ANCHOR, r"(?is)<a\b([^>]*)>(.*?)</a\s*>").replace_all(
        &text,
        |caps: &regex::Captures| {
            let href = attr(&caps[1], "href").unwrap_or_default();
            let inner = strip_tags(&caps[2]);
            let inner = inner.split_whitespace().collect::<Vec<_>>().join(" ");
            match (href.is_empty(), inner.is_empty()) {
                (true, _) => inner,
                (false, true) => format!("<{href}>"),
                (false, false) => format!("[{inner}]({href})"),
            }
        },
    );
    let text = regex(&SUMMARY, r"(?is)<summary\b[^>]*>(.*?)</summary\s*>").replace_all(
        &text,
        |caps: &regex::Captures| {
            let inner = strip_tags(&caps[1]);
            format!(
                "\n**▸ {}**\n",
                inner.split_whitespace().collect::<Vec<_>>().join(" ")
            )
        },
    );
    let text =
        regex(&HEADING, r"(?i)<h([1-6])\b[^>]*>").replace_all(&text, |caps: &regex::Captures| {
            let level = caps[1].parse::<usize>().unwrap_or(1);
            format!("\n{} ", "#".repeat(level))
        });
    let text = regex(&BREAK, r"(?i)<br\s*/?>").replace_all(&text, "\n");
    let text = regex(&RULE, r"(?i)<hr\b[^>]*>").replace_all(&text, "\n\n---\n\n");
    let text = regex(&ITEM, r"(?i)<li\b[^>]*>").replace_all(&text, "\n- ");
    let text = regex(
        &BLOCK,
        r"(?i)</?(?:p|div|details|ul|ol|table|tr|blockquote|section|center|h[1-6])\b[^>]*>",
    )
    .replace_all(&text, "\n");
    let text = regex(&STRONG, r"(?i)</?(?:b|strong)\s*>").replace_all(&text, "**");
    let text = regex(&EMPHASIS, r"(?i)</?(?:i|em)\s*>").replace_all(&text, "*");
    let text = regex(&STRIKE, r"(?i)</?(?:s|del|strike)\s*>").replace_all(&text, "~~");
    let text = regex(&CODE_TAG, r"(?i)</?(?:code|kbd|tt)\s*>").replace_all(&text, "`");
    let text = regex(&TAG, r"(?i)</?[a-z][a-z0-9-]*(?:\s[^<>]*)?/?>").replace_all(&text, "");
    // Lines left holding only whitespace by removed tags become blank.
    let mut reduced = String::with_capacity(text.len());
    for line in text.split('\n') {
        if line.trim().is_empty() {
            reduced.push('\n');
        } else {
            reduced.push_str(line);
            reduced.push('\n');
        }
    }
    let restored = reduced.trim_end_matches('\n').to_owned();
    if kept.is_empty() {
        return restored;
    }
    let mut result = String::with_capacity(restored.len());
    let mut rest = restored.as_str();
    while let Some(start) = rest.find('\u{1}') {
        result.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('\u{2}') else {
            rest = after;
            continue;
        };
        if let Some(code) = after[..end].parse::<usize>().ok().and_then(|i| kept.get(i)) {
            result.push_str(code);
        }
        rest = &after[end + 1..];
    }
    result.push_str(rest);
    result
}

fn strip_tags(text: &str) -> String {
    static TAG: OnceLock<Regex> = OnceLock::new();
    regex(&TAG, r"(?i)</?[a-z][a-z0-9-]*(?:\s[^<>]*)?/?>")
        .replace_all(text, "")
        .into_owned()
}

fn is_rule(line: &str) -> bool {
    let trimmed = line.trim();
    if line.len() - line.trim_start().len() > 3 {
        return false;
    }
    let mut chars = trimmed.chars().filter(|c| !c.is_whitespace());
    let Some(first) = chars.next() else {
        return false;
    };
    matches!(first, '-' | '*' | '_')
        && trimmed.chars().filter(|c| !c.is_whitespace()).count() >= 3
        && chars.all(|c| c == first)
}

fn heading(line: &str) -> Option<(u8, &str)> {
    let trimmed = line.trim_start();
    if line.len() - trimmed.len() > 3 {
        return None;
    }
    let level = trimmed.chars().take_while(|c| *c == '#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let rest = &trimmed[level..];
    if !rest.is_empty() && !rest.starts_with([' ', '\t']) {
        return None;
    }
    let text = rest.trim();
    // A closing run of `#` (after a space) is not part of the heading.
    let stripped = text.trim_end_matches('#');
    let text = if stripped.is_empty() || stripped.ends_with([' ', '\t']) {
        stripped.trim_end()
    } else {
        text
    };
    Some((level as u8, text))
}

/// `(indent, marker, content)` of a list item line.
fn list_item(line: &str) -> Option<(usize, String, &str)> {
    let trimmed = line.trim_start();
    let indent = line[..line.len() - trimmed.len()]
        .chars()
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum::<usize>();
    let mut chars = trimmed.char_indices();
    let (_, first) = chars.next()?;
    let (marker, rest) = if matches!(first, '-' | '*' | '+') {
        (first.to_string(), &trimmed[1..])
    } else {
        let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
        if digits == 0 || digits > 9 {
            return None;
        }
        let delimiter = trimmed[digits..].chars().next()?;
        if !matches!(delimiter, '.' | ')') {
            return None;
        }
        (format!("{}.", &trimmed[..digits]), &trimmed[digits + 1..])
    };
    if !(rest.is_empty() || rest.starts_with([' ', '\t'])) {
        return None;
    }
    Some((indent, marker, rest.trim_start()))
}

fn table_delimiter(line: &str) -> Option<Vec<Option<char>>> {
    let cells = table_cells(line);
    if cells.is_empty() || !line.contains('-') {
        return None;
    }
    cells
        .iter()
        .map(|cell| {
            let cell = cell.trim();
            let inner = cell.trim_start_matches(':').trim_end_matches(':');
            if inner.is_empty() || !inner.chars().all(|c| c == '-') {
                return None;
            }
            Some(match (cell.starts_with(':'), cell.ends_with(':')) {
                (true, true) => Some('c'),
                (false, true) => Some('r'),
                _ => None,
            })
        })
        .collect()
}

fn table_cells(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    let trimmed = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let trimmed = trimmed.strip_suffix('|').unwrap_or(trimmed);
    let mut cells = vec![String::new()];
    let mut escaped = false;
    let mut code = false;
    for c in trimmed.chars() {
        match c {
            '\\' if !escaped => {
                escaped = true;
                continue;
            }
            '`' => code = !code,
            '|' if !escaped && !code => {
                cells.push(String::new());
                continue;
            }
            _ => {}
        }
        if escaped && c != '|' {
            cells.last_mut().unwrap().push('\\');
        }
        escaped = false;
        cells.last_mut().unwrap().push(c);
    }
    cells
        .into_iter()
        .map(|cell| cell.trim().to_owned())
        .collect()
}

fn starts_block(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.is_empty()
        || heading(line).is_some()
        || is_rule(line)
        || trimmed.starts_with('>')
        || list_item(line).is_some()
}

fn push_blank(out: &mut Vec<MdLine>) {
    if out.last().is_some_and(|line| line.kind != BlockKind::Blank) {
        out.push(MdLine::new(BlockKind::Blank, Vec::new()));
    }
}

/// Block structure of HTML-reduced text.
fn blocks(text: &str, links: &mut Vec<String>, out: &mut Vec<MdLine>) {
    let lines = text.split('\n').collect::<Vec<_>>();
    let mut index = 0usize;
    // The content indent of the list item continuation lines hang under, when in a list.
    let mut list: Option<(Vec<MdSpan>, usize)> = None;
    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            push_blank(out);
            list = None;
            index += 1;
            continue;
        }
        // Setext headings: a text line underlined with `===` or `---`.
        if list.is_none() && !starts_block(line) {
            if let Some(next) = lines.get(index + 1) {
                let under = next.trim();
                let setext = !under.is_empty()
                    && next.len() - next.trim_start().len() <= 3
                    && (under.chars().all(|c| c == '=') || under.chars().all(|c| c == '-'));
                if setext && !line.contains('|') {
                    let level = if under.starts_with('=') { 1 } else { 2 };
                    out.push(MdLine::new(
                        BlockKind::Heading(level),
                        inline(trimmed, Inline::default(), links),
                    ));
                    index += 2;
                    continue;
                }
            }
        }
        if is_rule(line) {
            out.push(MdLine::new(BlockKind::Rule, Vec::new()));
            list = None;
            index += 1;
            continue;
        }
        if let Some((level, text)) = heading(line) {
            out.push(MdLine::new(
                BlockKind::Heading(level),
                inline(text, Inline::default(), links),
            ));
            list = None;
            index += 1;
            continue;
        }
        if trimmed.starts_with('>') {
            let mut quoted = String::new();
            while let Some(next) = lines.get(index) {
                let next_trimmed = next.trim_start();
                let Some(rest) = next_trimmed.strip_prefix('>') else {
                    break;
                };
                quoted.push_str(rest.strip_prefix(' ').unwrap_or(rest));
                quoted.push('\n');
                index += 1;
            }
            let mut inner = Vec::new();
            render_into(&quoted, links, &mut inner);
            while inner
                .last()
                .is_some_and(|line| line.kind == BlockKind::Blank)
            {
                inner.pop();
            }
            for mut line in inner {
                line.quote = line.quote.saturating_add(1);
                line.prefix.insert(0, marker("│ "));
                line.hang.insert(0, marker("│ "));
                out.push(line);
            }
            list = None;
            continue;
        }
        if trimmed.contains('|') {
            if let Some(alignment) = lines.get(index + 1).and_then(|next| table_delimiter(next)) {
                let header = table_cells(line);
                if header.len() == alignment.len() {
                    let mut rows = vec![header];
                    index += 2;
                    while let Some(next) = lines.get(index) {
                        if next.trim().is_empty() || !next.contains('|') {
                            break;
                        }
                        rows.push(table_cells(next));
                        index += 1;
                    }
                    table(&rows, &alignment, links, out);
                    list = None;
                    continue;
                }
            }
        }
        if let Some((indent, marker_text, content)) = list_item(line) {
            let depth = (indent / 2).min(6);
            let bullet = if marker_text.ends_with('.') {
                marker_text
            } else {
                ["•", "◦", "▪"][depth.min(2)].to_owned()
            };
            let (task, content) = if let Some(rest) = content.strip_prefix("[ ] ") {
                (Some("☐ "), rest)
            } else if let Some(rest) = content
                .strip_prefix("[x] ")
                .or_else(|| content.strip_prefix("[X] "))
            {
                (Some("☑ "), rest)
            } else {
                (None, content)
            };
            let lead = format!("{}{bullet} ", "  ".repeat(depth));
            let mut prefix = vec![marker(&lead)];
            if let Some(task) = task {
                prefix.push(marker(task));
            }
            let hang_width = spans_width(&prefix);
            let hang = vec![marker(&" ".repeat(hang_width))];
            let mut item = MdLine::new(
                BlockKind::ListItem,
                inline(content, Inline::default(), links),
            );
            item.prefix = prefix;
            item.hang = hang.clone();
            out.push(item);
            list = Some((hang, indent));
            index += 1;
            continue;
        }
        if let Some((hang, _)) = &list {
            // GitHub keeps a line break inside a list item; it hangs under the item text.
            let mut continued = MdLine::new(
                BlockKind::ListItem,
                inline(trimmed, Inline::default(), links),
            );
            continued.prefix = hang.clone();
            continued.hang = hang.clone();
            out.push(continued);
            index += 1;
            continue;
        }
        // Comments keep their line breaks (GitHub renders them as hard breaks).
        out.push(MdLine::new(
            BlockKind::Paragraph,
            inline(trimmed, Inline::default(), links),
        ));
        index += 1;
    }
}

fn table(
    rows: &[Vec<String>],
    alignment: &[Option<char>],
    links: &mut Vec<String>,
    out: &mut Vec<MdLine>,
) {
    let columns = alignment.len();
    let cells = rows
        .iter()
        .map(|row| {
            (0..columns)
                .map(|column| {
                    inline(
                        row.get(column).map_or("", String::as_str),
                        Inline::default(),
                        links,
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let widths = (0..columns)
        .map(|column| {
            cells
                .iter()
                .map(|row| spans_width(&row[column]))
                .max()
                .unwrap_or(0)
                .max(1)
        })
        .collect::<Vec<_>>();
    for (index, row) in cells.into_iter().enumerate() {
        let mut spans = Vec::new();
        for (column, cell) in row.into_iter().enumerate() {
            if column > 0 {
                spans.push(marker(" │ "));
            }
            let pad = widths[column].saturating_sub(spans_width(&cell));
            let (left, right) = match alignment[column] {
                Some('r') => (pad, 0),
                Some('c') => (pad / 2, pad - pad / 2),
                _ => (0, pad),
            };
            if left > 0 {
                spans.push(plain_span(&" ".repeat(left)));
            }
            spans.extend(cell);
            if right > 0 && column + 1 < columns {
                spans.push(plain_span(&" ".repeat(right)));
            }
        }
        out.push(MdLine::new(
            if index == 0 {
                BlockKind::TableHeader
            } else {
                BlockKind::TableRow
            },
            spans,
        ));
        if index == 0 {
            let rule = widths
                .iter()
                .map(|width| "─".repeat(*width))
                .collect::<Vec<_>>()
                .join("─┼─");
            out.push(MdLine::new(BlockKind::TableRule, vec![marker(&rule)]));
        }
    }
}

/// Decodes the HTML entities GitHub bodies commonly carry.
fn entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    static ENTITY: OnceLock<Regex> = OnceLock::new();
    regex(
        &ENTITY,
        r"&(#[0-9]{1,7}|#[xX][0-9a-fA-F]{1,6}|[a-zA-Z]{2,8});",
    )
    .replace_all(text, |caps: &regex::Captures| {
        let name = &caps[1];
        let decoded = if let Some(hex) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X"))
        {
            u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
        } else if let Some(decimal) = name.strip_prefix('#') {
            decimal.parse::<u32>().ok().and_then(char::from_u32)
        } else {
            match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                "mdash" => Some('—'),
                "ndash" => Some('–'),
                "hellip" => Some('…'),
                "copy" => Some('©'),
                _ => None,
            }
        };
        decoded.map_or_else(|| caps[0].to_owned(), |c| c.to_string())
    })
    .into_owned()
}

/// Pushes text in `style`, merging with the previous span of the same style.
fn push_text(out: &mut Vec<MdSpan>, text: &str, style: Inline) {
    if text.is_empty() {
        return;
    }
    let text = crate::app::safe_text(&if style.code {
        text.to_owned()
    } else {
        entities(text)
    });
    if let Some(last) = out.last_mut().filter(|last| last.style == style) {
        last.text.push_str(&text);
    } else {
        out.push(MdSpan { text, style });
    }
}

/// The byte index of the `]` closing the `[` at `open`, honouring nesting and escapes.
fn closing_bracket(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut escaped = false;
    for (offset, c) in text[open..].char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

/// `(url, end)` of a `(url "title")` destination starting at `open` (the `(`).
fn destination(text: &str, open: usize) -> Option<(String, usize)> {
    if !text[open..].starts_with('(') {
        return None;
    }
    let mut depth = 0usize;
    for (offset, c) in text[open..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    let inner = text[open + 1..open + offset].trim();
                    let url = inner
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .trim_start_matches('<')
                        .trim_end_matches('>');
                    return Some((url.to_owned(), open + offset + 1));
                }
            }
            '\n' => return None,
            _ => {}
        }
    }
    None
}

fn link_number(links: &mut Vec<String>, url: &str) -> usize {
    if let Some(index) = links.iter().position(|known| known == url) {
        return index + 1;
    }
    links.push(url.to_owned());
    links.len()
}

/// Inline Markdown of one line: code spans, images, links, autolinks, bare URLs, emphasis,
/// strikethrough, escapes and entities.
pub fn inline(text: &str, base: Inline, links: &mut Vec<String>) -> Vec<MdSpan> {
    let mut out = Vec::new();
    inline_into(text, base, links, &mut out);
    out
}

fn inline_into(text: &str, base: Inline, links: &mut Vec<String>, out: &mut Vec<MdSpan>) {
    let bytes = text.as_bytes();
    let mut plain_start = 0usize;
    let mut at = 0usize;
    let flush = |out: &mut Vec<MdSpan>, from: usize, to: usize| {
        if from < to {
            push_text(out, &text[from..to], base);
        }
    };
    while at < bytes.len() {
        let rest = &text[at..];
        let c = rest.chars().next().unwrap();
        // Escapes.
        if c == '\\' {
            if let Some(next) = rest[1..].chars().next().filter(char::is_ascii_punctuation) {
                flush(out, plain_start, at);
                push_text(out, &next.to_string(), base);
                at += 1 + next.len_utf8();
                plain_start = at;
                continue;
            }
        }
        // Code spans.
        if c == '`' {
            let run = rest.bytes().take_while(|b| *b == b'`').count();
            let fence = &rest[..run];
            if let Some(close) = rest[run..].find(fence) {
                let inner = &rest[run..run + close];
                let after_close = run + close + run;
                if !rest[after_close..].starts_with('`') {
                    flush(out, plain_start, at);
                    let inner = if inner.len() > 2 && inner.starts_with(' ') && inner.ends_with(' ')
                    {
                        &inner[1..inner.len() - 1]
                    } else {
                        inner
                    };
                    push_text(out, inner, Inline { code: true, ..base });
                    at += after_close;
                    plain_start = at;
                    continue;
                }
            }
            at += run;
            continue;
        }
        // Images and links.
        let image = rest.starts_with("![");
        if image || c == '[' {
            let open = at + usize::from(image);
            if let Some(close) = closing_bracket(text, open) {
                if let Some((url, end)) = destination(text, close + 1) {
                    flush(out, plain_start, at);
                    let label = &text[open + 1..close];
                    if image {
                        let alt = label.trim();
                        push_text(
                            out,
                            &if alt.is_empty() {
                                "[image]".to_owned()
                            } else {
                                format!("[image: {alt}]")
                            },
                            Inline {
                                image: true,
                                ..base
                            },
                        );
                    } else {
                        let style = Inline { link: true, ..base };
                        let before = out.len();
                        inline_into(label, style, links, out);
                        let shown: String = out[before..].iter().map(|s| s.text.as_str()).collect();
                        if !url.is_empty() && shown.trim() != url {
                            let number = link_number(links, &url);
                            out.push(MdSpan {
                                text: format!("[{number}]"),
                                style: Inline {
                                    marker: true,
                                    ..Inline::default()
                                },
                            });
                        }
                    }
                    at = end;
                    plain_start = at;
                    continue;
                }
            }
        }
        // Autolinks `<https://…>`.
        if c == '<' {
            if let Some(close) = rest.find('>') {
                let inner = &rest[1..close];
                if (inner.starts_with("http://") || inner.starts_with("https://"))
                    && !inner.contains(char::is_whitespace)
                {
                    flush(out, plain_start, at);
                    push_text(out, inner, Inline { link: true, ..base });
                    at += close + 1;
                    plain_start = at;
                    continue;
                }
            }
        }
        // Bare URLs at a word start.
        if (rest.starts_with("https://") || rest.starts_with("http://"))
            && !base.link
            && text[..at]
                .chars()
                .next_back()
                .is_none_or(|before| !before.is_alphanumeric())
        {
            let mut end = rest
                .find(|c: char| c.is_whitespace() || c == '<')
                .unwrap_or(rest.len());
            while end > 0 && rest[..end].ends_with(['.', ',', ';', ':', '!', '?', '\'', '"']) {
                end -= 1;
            }
            if rest[..end].ends_with(')') && !rest[..end].contains('(') {
                end -= 1;
            }
            flush(out, plain_start, at);
            push_text(out, &rest[..end], Inline { link: true, ..base });
            at += end;
            plain_start = at;
            continue;
        }
        // Emphasis and strikethrough: a delimiter run with a matching closer.
        if matches!(c, '*' | '_' | '~') {
            let run = rest.bytes().take_while(|b| *b == c as u8).count();
            let previous = text[..at].chars().next_back();
            let next = rest[run..].chars().next();
            let opens = next.is_some_and(|next| !next.is_whitespace())
                && (c != '_' || previous.is_none_or(|p| !p.is_alphanumeric()));
            let width = match (c, run) {
                ('~', 2..) => 2,
                ('~', _) => 0,
                (_, 3..) => 3,
                (_, n) => n,
            };
            if opens && width > 0 {
                let delimiter = &rest[..width];
                let search = &rest[width..];
                let mut found = None;
                let mut from = 0usize;
                while let Some(offset) = search[from..].find(delimiter) {
                    let position = from + offset;
                    let before = search[..position].chars().next_back();
                    let after = search[position + width..].chars().next();
                    let closes = position > 0
                        && before.is_some_and(|b| !b.is_whitespace())
                        && (c != '_' || after.is_none_or(|a| !a.is_alphanumeric()))
                        && !search[position + width..].starts_with(c);
                    if closes {
                        found = Some(position);
                        break;
                    }
                    from = position + delimiter.len();
                }
                if let Some(position) = found {
                    flush(out, plain_start, at);
                    let style = match (c, width) {
                        ('~', _) => Inline {
                            strike: true,
                            ..base
                        },
                        (_, 1) => Inline {
                            italic: true,
                            ..base
                        },
                        (_, 2) => Inline { bold: true, ..base },
                        _ => Inline {
                            bold: true,
                            italic: true,
                            ..base
                        },
                    };
                    inline_into(&search[..position], style, links, out);
                    at += width + position + width;
                    plain_start = at;
                    continue;
                }
            }
            at += run;
            continue;
        }
        at += c.len_utf8();
    }
    flush(out, plain_start, text.len());
}

#[cfg(test)]
#[path = "markdown_v114_tests.rs"]
mod tests;
