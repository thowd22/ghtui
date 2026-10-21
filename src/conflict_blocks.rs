//! Conflict-marker blocks in a working file, for resolving one block at a time.
//!
//! Git writes each unresolved region as `<<<<<<< ours`, an optional
//! `||||||| base` section (diff3/zdiff3), `=======` and `>>>>>>> theirs`.
//! Resolving a block replaces exactly that region with the chosen lines and
//! leaves every other byte of the file, and the index, untouched.

/// Which lines replace one conflict block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictBlockKeep {
    Ours,
    Theirs,
    /// Ours followed by theirs.
    Both,
    /// The common-ancestor lines; only present with diff3/zdiff3 markers.
    Base,
    /// Replacement text typed in ghtui.
    Edited,
}

impl ConflictBlockKeep {
    pub const NAMES: [&'static str; 5] = ["ours", "theirs", "both", "base", "edited"];
    pub fn parse(value: &str) -> Result<Self, String> {
        Ok(match value.trim() {
            "ours" => Self::Ours,
            "theirs" => Self::Theirs,
            "both" => Self::Both,
            "base" => Self::Base,
            "edited" => Self::Edited,
            _ => return Err("Keep must be ours, theirs, both, base or edited.".into()),
        })
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Ours => "ours",
            Self::Theirs => "theirs",
            Self::Both => "both (ours then theirs)",
            Self::Base => "base",
            Self::Edited => "edited text",
        }
    }
}

/// One parsed conflict region. Line numbers are 1-based in the working file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictBlock {
    pub start_line: usize,
    pub end_line: usize,
    /// Byte range of the whole region, markers included.
    pub range: std::ops::Range<usize>,
    pub ours_label: String,
    pub theirs_label: String,
    pub ours: Vec<u8>,
    pub base: Option<Vec<u8>>,
    pub theirs: Vec<u8>,
}

impl ConflictBlock {
    pub fn summary(&self, number: usize, total: usize) -> String {
        format!(
            "Block {number} of {total} (lines {}–{}): ours {} line{} • theirs {} line{}{}",
            self.start_line,
            self.end_line,
            line_count(&self.ours),
            plural(line_count(&self.ours)),
            line_count(&self.theirs),
            plural(line_count(&self.theirs)),
            match &self.base {
                Some(base) => format!(
                    " • base {} line{}",
                    line_count(base),
                    plural(line_count(base))
                ),
                None => String::new(),
            }
        )
    }
}

fn plural(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}

pub fn line_count(bytes: &[u8]) -> usize {
    if bytes.is_empty() {
        0
    } else {
        bytes.split_inclusive(|byte| *byte == b'\n').count()
    }
}

fn marker(line: &[u8], byte: u8) -> Option<&[u8]> {
    let body = line.strip_suffix(b"\n").unwrap_or(line);
    let body = body.strip_suffix(b"\r").unwrap_or(body);
    if body.len() < 7 || body[..7].iter().any(|item| *item != byte) {
        return None;
    }
    let rest = &body[7..];
    match rest.first() {
        None => Some(rest),
        Some(b' ') => Some(&rest[1..]),
        _ => None,
    }
}

/// Parses every complete conflict region. An unterminated or malformed region
/// is an error, so a half-edited file is never rewritten by block number.
pub fn parse_conflict_blocks(content: &[u8]) -> Result<Vec<ConflictBlock>, String> {
    #[derive(PartialEq)]
    enum Part {
        Ours,
        Base,
        Theirs,
    }
    let mut blocks = Vec::new();
    let mut current: Option<(ConflictBlock, Part)> = None;
    let mut offset = 0;
    for (index, line) in content.split_inclusive(|byte| *byte == b'\n').enumerate() {
        let number = index + 1;
        let start = offset;
        offset += line.len();
        match current.as_mut() {
            None => {
                if let Some(label) = marker(line, b'<') {
                    current = Some((
                        ConflictBlock {
                            start_line: number,
                            end_line: number,
                            range: start..start,
                            ours_label: crate::git::display_bytes(label),
                            theirs_label: String::new(),
                            ours: Vec::new(),
                            base: None,
                            theirs: Vec::new(),
                        },
                        Part::Ours,
                    ));
                }
                // Outside a block, `=======` and friends are ordinary content
                // (for example a Markdown heading underline).
            }
            Some((block, part)) => {
                if marker(line, b'<').is_some() {
                    return Err(format!(
                        "Line {number} opens a conflict block inside block starting at line {}; nested markers are not resolved by block.",
                        block.start_line
                    ));
                } else if *part == Part::Ours && marker(line, b'|').is_some() {
                    block.base = Some(Vec::new());
                    *part = Part::Base;
                } else if *part != Part::Theirs && marker(line, b'=').is_some_and(<[u8]>::is_empty)
                {
                    *part = Part::Theirs;
                } else if *part == Part::Theirs && marker(line, b'>').is_some() {
                    let (mut block, _) = current.take().unwrap();
                    block.theirs_label = crate::git::display_bytes(marker(line, b'>').unwrap());
                    block.end_line = number;
                    block.range.end = offset;
                    blocks.push(block);
                } else {
                    match part {
                        Part::Ours => block.ours.extend_from_slice(line),
                        Part::Base => block
                            .base
                            .get_or_insert_with(Vec::new)
                            .extend_from_slice(line),
                        Part::Theirs => block.theirs.extend_from_slice(line),
                    }
                }
            }
        }
    }
    if let Some((block, _)) = current {
        return Err(format!(
            "Conflict block starting at line {} is not closed with >>>>>>>; fix the file outside ghtui or choose a whole-file version.",
            block.start_line
        ));
    }
    Ok(blocks)
}

/// The replacement lines for one block.
pub fn block_replacement(
    block: &ConflictBlock,
    keep: ConflictBlockKeep,
    text: &str,
) -> Result<Vec<u8>, String> {
    Ok(match keep {
        ConflictBlockKeep::Ours => block.ours.clone(),
        ConflictBlockKeep::Theirs => block.theirs.clone(),
        ConflictBlockKeep::Both => {
            let mut both = block.ours.clone();
            if !both.is_empty() && !both.ends_with(b"\n") {
                both.push(b'\n');
            }
            both.extend_from_slice(&block.theirs);
            both
        }
        ConflictBlockKeep::Base => block.base.clone().ok_or(
            "This block has no base lines (markers were written without merge.conflictStyle=diff3); keep ours, theirs, both or edited text instead.",
        )?,
        ConflictBlockKeep::Edited => {
            if text.contains('\0') {
                return Err("Edited text cannot contain NUL.".into());
            }
            let marked = text.split('\n').any(|line| {
                let line = line.as_bytes();
                marker(line, b'<').is_some()
                    || marker(line, b'>').is_some()
                    || marker(line, b'|').is_some()
                    || marker(line, b'=').is_some_and(<[u8]>::is_empty)
            });
            if marked {
                return Err("Edited text still contains conflict markers.".into());
            }
            let mut bytes = text.as_bytes().to_vec();
            if !bytes.is_empty() && !bytes.ends_with(b"\n") {
                bytes.push(b'\n');
            }
            bytes
        }
    })
}

/// The whole file after resolving block `number` (1-based), plus the count of
/// blocks that remain.
pub fn resolve_conflict_block(
    content: &[u8],
    number: usize,
    keep: ConflictBlockKeep,
    text: &str,
) -> Result<(Vec<u8>, usize), String> {
    let blocks = parse_conflict_blocks(content)?;
    if blocks.is_empty() {
        return Err("The working file has no conflict-marker blocks left; Stage it, or choose a whole-file version.".into());
    }
    let block = blocks
        .get(number.wrapping_sub(1))
        .ok_or_else(|| format!("Block must be a number from 1 to {}.", blocks.len()))?;
    let replacement = block_replacement(block, keep, text)?;
    let mut result = content[..block.range.start].to_vec();
    result.extend_from_slice(&replacement);
    result.extend_from_slice(&content[block.range.end..]);
    Ok((result, blocks.len() - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &[u8] = b"top\n<<<<<<< HEAD\nours one\n=======\ntheirs one\n>>>>>>> feature\nmiddle\n<<<<<<< HEAD\nours two\n||||||| base\nbase two\n=======\ntheirs two\n>>>>>>> feature\nend\n";

    #[test]
    fn v42_conflict_blocks_parse_labels_lines_and_base() {
        let blocks = parse_conflict_blocks(FILE).unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!((blocks[0].start_line, blocks[0].end_line), (2, 6));
        assert_eq!(blocks[0].ours_label, "HEAD");
        assert_eq!(blocks[0].theirs_label, "feature");
        assert_eq!(blocks[0].ours, b"ours one\n");
        assert_eq!(blocks[0].base, None);
        assert_eq!(blocks[1].base.as_deref(), Some(&b"base two\n"[..]));
        assert_eq!(
            blocks[1].summary(2, 2),
            "Block 2 of 2 (lines 8–14): ours 1 line • theirs 1 line • base 1 line"
        );
    }

    #[test]
    fn v42_conflict_blocks_resolve_one_block_and_keep_the_rest() {
        let (theirs, left) =
            resolve_conflict_block(FILE, 1, ConflictBlockKeep::Theirs, "").unwrap();
        assert_eq!(left, 1);
        assert!(theirs.starts_with(b"top\ntheirs one\nmiddle\n<<<<<<< HEAD\n"));
        let (base, left) = resolve_conflict_block(&theirs, 1, ConflictBlockKeep::Base, "").unwrap();
        assert_eq!(left, 0);
        assert_eq!(base, b"top\ntheirs one\nmiddle\nbase two\nend\n");
        let (both, _) = resolve_conflict_block(FILE, 1, ConflictBlockKeep::Both, "").unwrap();
        assert!(both.starts_with(b"top\nours one\ntheirs one\nmiddle\n"));
        let (edited, _) =
            resolve_conflict_block(FILE, 1, ConflictBlockKeep::Edited, "mine a\nmine b").unwrap();
        assert!(edited.starts_with(b"top\nmine a\nmine b\nmiddle\n"));
    }

    #[test]
    fn v42_conflict_blocks_refuse_bad_numbers_markers_and_missing_base() {
        assert!(resolve_conflict_block(FILE, 0, ConflictBlockKeep::Ours, "").is_err());
        assert!(resolve_conflict_block(FILE, 3, ConflictBlockKeep::Ours, "")
            .unwrap_err()
            .contains("1 to 2"));
        assert!(resolve_conflict_block(FILE, 1, ConflictBlockKeep::Base, "")
            .unwrap_err()
            .contains("no base lines"));
        assert!(
            resolve_conflict_block(FILE, 1, ConflictBlockKeep::Edited, "a\n=======\nb")
                .unwrap_err()
                .contains("markers")
        );
        assert!(parse_conflict_blocks(b"<<<<<<< HEAD\nours\n=======\n").is_err());
        assert!(parse_conflict_blocks(b"Title\n=======\n")
            .unwrap()
            .is_empty());
        assert!(
            resolve_conflict_block(b"clean\n", 1, ConflictBlockKeep::Ours, "")
                .unwrap_err()
                .contains("no conflict-marker blocks")
        );
        // A line that only looks like a marker prefix is content.
        assert!(parse_conflict_blocks(b"<<<<<<<<< not a marker\n")
            .unwrap()
            .is_empty());
    }
}
