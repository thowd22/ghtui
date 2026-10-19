//! Test-only control label model shared by PTY journeys and synthetic-frame
//! contracts. It depends only on decoded vt100 cells, so contract crates can
//! include it by path. Labels are input hints, never application identity.
#![allow(dead_code)]

/// The one source for a view's focused title. Breadcrumb chrome changes only
/// this text, not every journey that asserts which view owns focus.
pub fn view_title(view: &str) -> String {
    format!("{view} • focused view")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// Square and rounded box corners are the same observed border.
pub fn is_corner(cell: &str, corner: Corner) -> bool {
    match corner {
        Corner::TopLeft => matches!(cell, "┌" | "╭"),
        Corner::TopRight => matches!(cell, "┐" | "╮"),
        Corner::BottomLeft => matches!(cell, "└" | "╰"),
        Corner::BottomRight => matches!(cell, "┘" | "╯"),
    }
}

pub fn char_is_corner(character: char, corner: Corner) -> bool {
    let mut buffer = [0; 4];
    is_corner(character.encode_utf8(&mut buffer), corner)
}

pub fn cell_is_corner(screen: &vt100::Screen, row: u16, column: u16, corner: Corner) -> bool {
    screen
        .cell(row, column)
        .is_some_and(|cell| is_corner(&cell.contents(), corner))
}

/// One painted action label: `[l Logs]` is key `l` and text `Logs`. Labels
/// without a recognisable leading key, such as `[Confirm action]`, have none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Control {
    pub key: Option<String>,
    pub text: String,
    pub bracketed: bool,
}

impl Control {
    pub fn parse(label: &str) -> Self {
        let bracketed = label.len() > 2
            && label.starts_with('[')
            && label.ends_with(']')
            && label[1..label.len() - 1]
                .chars()
                .all(|character| !character.is_control() && !matches!(character, '[' | ']'));
        let inner = if bracketed {
            &label[1..label.len() - 1]
        } else {
            label
        };
        match inner.split_once(' ') {
            Some((key, text)) if !text.trim().is_empty() && key_token_bytes(key).is_some() => {
                Self {
                    key: Some(key.to_owned()),
                    text: text.to_owned(),
                    bracketed,
                }
            }
            _ => Self {
                key: None,
                text: inner.to_owned(),
                bracketed,
            },
        }
    }

    /// The unbracketed key-bar form, for example `l Logs`.
    pub fn footer_form(&self) -> String {
        match &self.key {
            Some(key) => format!("{key} {}", self.text),
            None => self.text.clone(),
        }
    }

    pub fn key_bytes(&self) -> Option<Vec<u8>> {
        key_token_bytes(self.key.as_deref()?)
    }
}

/// Terminal input bytes for a painted key token, matching what journeys send.
pub fn key_token_bytes(key: &str) -> Option<Vec<u8>> {
    let named = |name: &str| -> Option<Vec<u8>> {
        Some(
            match name.to_ascii_lowercase().as_str() {
                "esc" => "\x1b",
                "enter" => "\r",
                "tab" => "\t",
                "shift-tab" | "backtab" => "\x1b[Z",
                "space" => " ",
                "backspace" => "\x7f",
                "pgup" => "\x1b[5~",
                "pgdn" => "\x1b[6~",
                "home" => "\x1b[H",
                "end" => "\x1b[F",
                "del" | "delete" => "\x1b[3~",
                "up" | "↑" => "\x1b[A",
                "down" | "↓" => "\x1b[B",
                "right" | "→" => "\x1b[C",
                "left" | "←" => "\x1b[D",
                "f1" => "\x1bOP",
                "f2" => "\x1bOQ",
                "f3" => "\x1bOR",
                "f4" => "\x1bOS",
                "f5" => "\x1b[15~",
                "f6" => "\x1b[17~",
                "f7" => "\x1b[18~",
                "f8" => "\x1b[19~",
                "f9" => "\x1b[20~",
                "f10" => "\x1b[21~",
                "f11" => "\x1b[23~",
                "f12" => "\x1b[24~",
                _ => return None,
            }
            .as_bytes()
            .to_vec(),
        )
    };
    if let Some(bytes) = named(key) {
        return Some(bytes);
    }
    let mut characters = key.chars();
    if let (Some(character), None) = (characters.next(), characters.next()) {
        return (!character.is_control() && !character.is_whitespace())
            .then(|| character.to_string().into_bytes());
    }
    if let Some(rest) = key.strip_prefix("Ctrl-") {
        let mut letters = rest.chars();
        if let (Some(letter), None) = (letters.next(), letters.next()) {
            return letter
                .is_ascii_alphabetic()
                .then(|| vec![letter.to_ascii_uppercase() as u8 & 0x1f]);
        }
        return Some(
            match rest.to_ascii_lowercase().as_str() {
                "pgup" => "\x1b[5;5~",
                "pgdn" => "\x1b[6;5~",
                "enter" => "\x1b[13;5u",
                _ => return None,
            }
            .as_bytes()
            .to_vec(),
        );
    }
    let rest = key.strip_prefix("Alt-")?;
    let mut letters = rest.chars();
    match (letters.next(), letters.next()) {
        (Some(letter), None) if !letter.is_control() && !letter.is_whitespace() => {
            Some(format!("\x1b{letter}").into_bytes())
        }
        _ => None,
    }
}

/// Decoded row text plus the byte offset at which each non-continuation cell
/// starts. Wide glyph continuations and combining marks never shift columns.
pub fn row_text(screen: &vt100::Screen, row: u16) -> Option<(String, Vec<(usize, u16)>)> {
    let mut line = String::new();
    let mut starts = Vec::new();
    for column in 0..screen.size().1 {
        let cell = screen.cell(row, column)?;
        if cell.is_wide_continuation() {
            continue;
        }
        starts.push((line.len(), column));
        let contents = cell.contents();
        if contents.is_empty() {
            line.push(' ');
        } else {
            line.push_str(&contents);
        }
    }
    Some((line, starts))
}

fn column_at(starts: &[(usize, u16)], byte: usize) -> Option<u16> {
    starts
        .iter()
        .rev()
        .find(|(start, _)| *start <= byte)
        .map(|(_, column)| *column)
}

/// First plain-text match in decoded cells, top to bottom.
pub fn locate_text(screen: &vt100::Screen, text: &str) -> Option<(u16, u16)> {
    for row in 0..screen.size().0 {
        let (line, starts) = row_text(screen, row)?;
        if let Some(byte) = line.find(text) {
            return column_at(&starts, byte).map(|column| (column, row));
        }
    }
    None
}

/// A whole `key text` token in one row, ignoring ASCII case. Key-bar items
/// are separated by two spaces, so a token starts after a space or the row
/// edge and ends at two spaces or blank row tail: `l Logs` never matches in
/// `all Logs`, and `R Rerun` never matches the start of `R Rerun failed`.
pub fn locate_token_in_row(screen: &vt100::Screen, row: u16, token: &str) -> Option<(u16, u16)> {
    if token.trim().is_empty() {
        return None;
    }
    let (line, starts) = row_text(screen, row)?;
    let haystack = line.to_ascii_lowercase();
    let needle = token.to_ascii_lowercase();
    for (byte, _) in haystack.match_indices(&needle) {
        let before = haystack[..byte].chars().next_back();
        let after = &haystack[byte + needle.len()..];
        if before.is_none_or(|c| c == ' ') && (after.starts_with("  ") || after.trim().is_empty()) {
            return column_at(&starts, byte).map(|column| (column, row));
        }
    }
    None
}

pub fn footer_row(screen: &vt100::Screen) -> u16 {
    screen.size().0.saturating_sub(1)
}

/// A bracketed label is found exactly anywhere first. Otherwise only the key
/// bar (the last row) may supply its `key text` token form, so body prose with
/// the same words cannot become a click target. Unbracketed labels keep plain
/// text semantics.
pub fn locate_control(screen: &vt100::Screen, label: &str) -> Option<(u16, u16)> {
    let control = Control::parse(label);
    if !control.bracketed {
        return locate_text(screen, label);
    }
    locate_text(screen, label).or_else(|| footer_control(screen, &control))
}

/// Only the footer token form, never an exact bracketed body match. Below 100 columns the bar
/// paints compact labels (`: Cmds`, `D Dispatch`), so a missing full token falls back to the
/// one token with the same key whose compact caption starts like the full one: the bar binds
/// each key to exactly one control.
pub fn footer_control(screen: &vt100::Screen, control: &Control) -> Option<(u16, u16)> {
    let row = footer_row(screen);
    locate_token_in_row(screen, row, &control.footer_form()).or_else(|| {
        if screen.size().1 >= 100 {
            return None;
        }
        let key = control.key.as_deref()?;
        let initial = control.text.chars().next()?.to_ascii_lowercase();
        let (line, starts) = row_text(screen, row)?;
        let prefix = format!("{key} ");
        let mut found = None;
        let mut offset = 0;
        for token in line.split("  ") {
            let at = offset;
            offset += token.len() + 2;
            let trimmed = token.trim_start();
            let caption = trimmed.get(prefix.len()..).unwrap_or_default();
            // Only a shorter (compact) caption stands in: `R rerun failed` is not `[R Rerun]`.
            if trimmed.starts_with(&prefix)
                && caption.trim_end().len() < control.text.len()
                && caption
                    .chars()
                    .next()
                    .is_some_and(|first| first.to_ascii_lowercase() == initial)
            {
                if found.is_some() {
                    return None;
                }
                found = column_at(&starts, at + token.len() - trimmed.len()).map(|x| (x, row));
            }
        }
        found
    })
}

/// The restored main key bar: the Global tail tokens `? Help` and `q Quit` in the actual last
/// row. A modal's bracketed `[q Quit]` row is not the main key bar.
pub fn main_footer_visible(screen: &vt100::Screen) -> bool {
    let row = footer_row(screen);
    locate_token_in_row(screen, row, "? Help").is_some()
        && locate_token_in_row(screen, row, "q Quit").is_some()
}
