//! Bounded patch-local paint. No full-file lexical-state claim is made.
use super::{NativeDiffPaintRoleV1 as Role, NativeDiffSpanV1, WithinPair};
use crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy)]
pub(super) enum Language {
    Rust,
    Json,
    Toml,
    Shell,
    Plain,
}
impl Language {
    pub(super) fn for_path(path: Option<&[u8]>) -> Self {
        match path.and_then(|path| path.rsplit(|byte| *byte == b'.').next()) {
            Some(b"rs") => Self::Rust,
            Some(b"json") => Self::Json,
            Some(b"toml") => Self::Toml,
            Some(b"sh" | b"bash") => Self::Shell,
            _ => Self::Plain,
        }
    }
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Rust => "Rust syntax: partial patch-local state; full-side context not loaded",
            Self::Json => "JSON syntax: partial patch-local state; full-side context not loaded",
            Self::Toml => "TOML syntax: partial patch-local state; full-side context not loaded",
            Self::Shell => "Shell syntax: partial patch-local state; full-side context not loaded",
            Self::Plain => "Plain text: unsupported syntax; original patch text",
        }
    }
}
#[derive(Default)]
pub(super) struct Lexer {
    block_comment: bool,
    quote: Option<u8>,
}
impl Lexer {
    pub(super) fn spans(
        &mut self,
        raw: &[u8],
        language: Language,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<Vec<(Range<usize>, Role)>, String> {
        if matches!(language, Language::Plain) || std::str::from_utf8(raw).is_err() {
            return Ok(Vec::new());
        }
        let mut spans = Vec::new();
        let mut i = 0;
        while i < raw.len() {
            operation.check()?;
            let start = i;
            if self.block_comment {
                while i < raw.len() && !raw[i..].starts_with(b"*/") {
                    i += 1;
                }
                if i < raw.len() {
                    i += 2;
                    self.block_comment = false;
                }
                spans.push((start..i, Role::Comment));
            } else if let Some(quote) = self.quote {
                while i < raw.len() {
                    if raw[i] == b'\\' {
                        i = (i + 2).min(raw.len());
                    } else if raw[i] == quote {
                        i += 1;
                        self.quote = None;
                        break;
                    } else {
                        i += 1;
                    }
                }
                spans.push((start..i, Role::String));
            } else if matches!(language, Language::Rust) && raw[i..].starts_with(b"/*") {
                self.block_comment = true;
                // The next iteration paints the entire comment including opener.
            } else if (matches!(language, Language::Rust) && raw[i..].starts_with(b"//"))
                || (matches!(language, Language::Toml | Language::Shell) && raw[i] == b'#')
            {
                spans.push((i..raw.len(), Role::Comment));
                break;
            } else if raw[i] == b'"'
                || (matches!(language, Language::Shell | Language::Toml) && raw[i] == b'\'')
            {
                let quote = raw[i];
                i += 1;
                self.quote = Some(quote);
                while i < raw.len() {
                    if raw[i] == b'\\' {
                        i = (i + 2).min(raw.len());
                    } else if raw[i] == quote {
                        i += 1;
                        self.quote = None;
                        break;
                    } else {
                        i += 1;
                    }
                }
                spans.push((start..i, Role::String));
            } else if raw[i].is_ascii_digit() {
                i += 1;
                while i < raw.len()
                    && (raw[i].is_ascii_alphanumeric() || matches!(raw[i], b'.' | b'_'))
                {
                    i += 1;
                }
                spans.push((start..i, Role::Number));
            } else if raw[i].is_ascii_alphabetic() || raw[i] == b'_' {
                i += 1;
                while i < raw.len() && (raw[i].is_ascii_alphanumeric() || raw[i] == b'_') {
                    i += 1;
                }
                if matches!(
                    &raw[start..i],
                    b"fn"
                        | b"let"
                        | b"mut"
                        | b"pub"
                        | b"use"
                        | b"mod"
                        | b"impl"
                        | b"struct"
                        | b"enum"
                        | b"match"
                        | b"if"
                        | b"else"
                        | b"return"
                        | b"true"
                        | b"false"
                        | b"null"
                        | b"const"
                        | b"for"
                        | b"while"
                        | b"in"
                        | b"do"
                        | b"done"
                        | b"then"
                        | b"fi"
                        | b"export"
                ) {
                    spans.push((start..i, Role::Keyword));
                }
            } else {
                i += 1;
            }
        }
        operation.check()?;
        Ok(spans)
    }
}

pub(super) fn within_line(
    old: &[u8],
    new: &[u8],
    operation: &LocalGitNavigationReadV1,
) -> Result<WithinPair, String> {
    operation.check()?;
    let (Ok(old_text), Ok(new_text)) = (std::str::from_utf8(old), std::str::from_utf8(new)) else {
        return fallback(old, new, operation);
    };
    if old.len() > 4096 || new.len() > 4096 {
        return fallback(old, new, operation);
    }
    let a = old_text
        .grapheme_indices(true)
        .take(257)
        .collect::<Vec<_>>();
    let b = new_text
        .grapheme_indices(true)
        .take(257)
        .collect::<Vec<_>>();
    if a.len() > 256 || b.len() > 256 || a.len().saturating_mul(b.len()) > 65_536 {
        return fallback(old, new, operation);
    }
    let columns = b.len() + 1;
    let mut lcs = vec![0u16; (a.len() + 1) * columns];
    for i in (0..a.len()).rev() {
        operation.check()?;
        for j in (0..b.len()).rev() {
            lcs[i * columns + j] = if a[i].1 == b[j].1 {
                1 + lcs[(i + 1) * columns + j + 1]
            } else {
                lcs[(i + 1) * columns + j].max(lcs[i * columns + j + 1])
            };
        }
    }
    let mut old_ranges = Vec::new();
    let mut new_ranges = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        operation.check()?;
        if i < a.len() && j < b.len() && a[i].1 == b[j].1 {
            i += 1;
            j += 1;
        } else if i < a.len()
            && (j == b.len() || lcs[(i + 1) * columns + j] >= lcs[i * columns + j + 1])
        {
            append_range(&mut old_ranges, a[i].0..a[i].0 + a[i].1.len());
            i += 1;
        } else {
            append_range(&mut new_ranges, b[j].0..b[j].0 + b[j].1.len());
            j += 1;
        }
    }
    operation.check()?;
    Ok((old_ranges, new_ranges, false))
}
fn append_range(ranges: &mut Vec<Range<usize>>, next: Range<usize>) {
    if let Some(previous) = ranges
        .last_mut()
        .filter(|previous| previous.end == next.start)
    {
        previous.end = next.end;
    } else {
        ranges.push(next);
    }
}
fn fallback(
    old: &[u8],
    new: &[u8],
    operation: &LocalGitNavigationReadV1,
) -> Result<WithinPair, String> {
    let mut prefix = 0;
    while prefix < old.len().min(new.len()) && old[prefix] == new[prefix] {
        if prefix % 256 == 0 {
            operation.check()?;
        }
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < old.len().min(new.len()) - prefix
        && old[old.len() - 1 - suffix] == new[new.len() - 1 - suffix]
    {
        if suffix % 256 == 0 {
            operation.check()?;
        }
        suffix += 1;
    }
    operation.check()?;
    let a = (prefix < old.len() - suffix)
        .then_some(prefix..old.len() - suffix)
        .into_iter()
        .collect();
    let b = (prefix < new.len() - suffix)
        .then_some(prefix..new.len() - suffix)
        .into_iter()
        .collect();
    Ok((a, b, true))
}

/// Escape once while mapping only requested paint endpoints, not an unbounded
/// byte->display table. Ranges touching a Unicode/control unit paint that whole
/// safe unit, and never change raw canonical bytes or click identity.
pub(super) fn escape(
    raw: &[u8],
    spans: &[(Range<usize>, Role)],
    mark: bool,
    operation: &LocalGitNavigationReadV1,
) -> Result<(String, Vec<NativeDiffSpanV1>), String> {
    operation.check()?;
    let mut endpoints = Vec::with_capacity(spans.len() * 2);
    let mut mapped = vec![(0, 0); spans.len()];
    for (index, (bytes, _)) in spans.iter().enumerate() {
        if bytes.start > bytes.end || bytes.end > raw.len() {
            return Err("Native diff paint range is outside original raw line".into());
        }
        if bytes.is_empty() {
            continue;
        }
        endpoints.push((bytes.start, false, index));
        endpoints.push((bytes.end, true, index));
    }
    endpoints.sort_unstable();
    let mut endpoint = 0;
    let mut output = String::new();
    let mut offset = 0;
    let mut unit = |character: char, raw_len: usize| -> Result<(), String> {
        operation.check()?;
        let start = output.len();
        match character {
            ' ' if mark => output.push('·'),
            '\t' if mark => output.push('→'),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            c if c.is_control()
                || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') =>
            {
                use std::fmt::Write;
                write!(&mut output, "\\u{{{:x}}}", c as u32)
                    .map_err(|_| "Native diff safe text formatting failed")?;
            }
            c => output.push(c),
        }
        while endpoint < endpoints.len() && endpoints[endpoint].0 <= offset + raw_len {
            let (position, end, index) = endpoints[endpoint];
            let display = if position == offset {
                start
            } else if position == offset + raw_len || end {
                output.len()
            } else {
                start
            };
            if end {
                mapped[index].1 = display;
            } else {
                mapped[index].0 = display;
            }
            endpoint += 1;
        }
        offset += raw_len;
        Ok(())
    };
    let mut remaining = raw;
    while !remaining.is_empty() {
        match std::str::from_utf8(remaining) {
            Ok(text) => {
                for character in text.chars() {
                    unit(character, character.len_utf8())?;
                }
                break;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                for character in std::str::from_utf8(&remaining[..valid])
                    .expect("UTF8 prefix")
                    .chars()
                {
                    unit(character, character.len_utf8())?;
                }
                remaining = &remaining[valid..];
                let bad = error.error_len().unwrap_or(remaining.len());
                unit('\u{fffd}', bad)?;
                remaining = &remaining[bad..];
            }
        }
    }
    let mapped: Vec<_> = mapped
        .into_iter()
        .zip(spans)
        .filter_map(|((start, end), (_, role))| {
            (start < end).then_some(NativeDiffSpanV1 {
                bytes: start..end,
                role: *role,
            })
        })
        .collect();
    operation.check()?;
    Ok((output, resolve_spans(mapped, operation)?))
}

// Resolve paint precedence ONCE in the producing worker. Drawing borrows these
// disjoint UTF-8 slices; it does not perform syntax/search/within-line matching.
fn resolve_spans(
    spans: Vec<NativeDiffSpanV1>,
    operation: &LocalGitNavigationReadV1,
) -> Result<Vec<NativeDiffSpanV1>, String> {
    let roles = [
        Role::Number,
        Role::Keyword,
        Role::String,
        Role::Comment,
        Role::WithinLine,
        Role::Search,
    ];
    let mut events = Vec::with_capacity(spans.len().saturating_mul(2));
    for span in spans {
        operation.check()?;
        let rank = roles
            .iter()
            .position(|role| *role == span.role)
            .expect("closed paint role");
        events.push((span.bytes.start, true, rank));
        events.push((span.bytes.end, false, rank));
    }
    events.sort_unstable();
    operation.check()?;
    let mut active = [0usize; 6];
    let mut output: Vec<NativeDiffSpanV1> = Vec::new();
    let mut index = 0;
    while index < events.len() {
        operation.check()?;
        let start = events[index].0;
        while index < events.len() && events[index].0 == start {
            let (_, begin, rank) = events[index];
            if begin {
                active[rank] += 1;
            } else {
                active[rank] -= 1;
            }
            index += 1;
        }
        let Some(end) = events.get(index).map(|event| event.0) else {
            break;
        };
        if let Some(rank) = active.iter().rposition(|count| *count != 0) {
            let role = roles[rank];
            if let Some(last) = output
                .last_mut()
                .filter(|last| last.role == role && last.bytes.end == start)
            {
                last.bytes.end = end;
            } else {
                output.push(NativeDiffSpanV1 {
                    bytes: start..end,
                    role,
                });
            }
        }
    }
    operation.check()?;
    Ok(output)
}
