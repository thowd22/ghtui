//! TASK-121: the one fuzzy ranking every type-to-filter finder shares (the TASK-116 log finder
//! and the Search view). Nucleo with smart case; a text that holds every typed word as written
//! ranks before scattered fuzzy letters, like a finder user expects.
use nucleo_matcher::{
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
    Matcher, Utf32Str,
};

/// How well one text matched: `whole` when it contains every typed word (smart case), then
/// nucleo's score. Sort with [`FuzzyMatchV1::better`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FuzzyMatchV1 {
    pub whole: bool,
    pub score: u32,
}
impl FuzzyMatchV1 {
    /// Best first: whole-word matches, then the higher score.
    pub fn better(&self, other: &Self) -> std::cmp::Ordering {
        other
            .whole
            .cmp(&self.whole)
            .then(other.score.cmp(&self.score))
    }
}

pub struct FuzzyRankerV1 {
    pattern: Pattern,
    matcher: Matcher,
    buf: Vec<char>,
    sensitive: bool,
    words: Vec<String>,
}
impl FuzzyRankerV1 {
    pub fn new(query: &str) -> Self {
        let sensitive = query.chars().any(char::is_uppercase);
        Self {
            pattern: Pattern::new(
                query,
                CaseMatching::Smart,
                Normalization::Smart,
                AtomKind::Fuzzy,
            ),
            matcher: Matcher::new(nucleo_matcher::Config::DEFAULT),
            buf: Vec::new(),
            sensitive,
            words: query
                .split_whitespace()
                .map(|word| {
                    if sensitive {
                        word.to_owned()
                    } else {
                        word.to_lowercase()
                    }
                })
                .collect(),
        }
    }

    /// Whether the query has anything to match.
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    fn folded<'t>(&self, text: &'t str) -> std::borrow::Cow<'t, str> {
        if self.sensitive {
            std::borrow::Cow::Borrowed(text)
        } else {
            std::borrow::Cow::Owned(text.to_lowercase())
        }
    }

    /// `None` when `text` does not match at all.
    pub fn rank(&mut self, text: &str) -> Option<FuzzyMatchV1> {
        if self.words.is_empty() {
            return None;
        }
        let score = self
            .pattern
            .score(Utf32Str::new(text, &mut self.buf), &mut self.matcher)?;
        let folded = self.folded(text);
        let whole = self.words.iter().all(|word| folded.contains(word.as_str()));
        Some(FuzzyMatchV1 { whole, score })
    }

    /// Sorted, unique char indices of nucleo's matched characters.
    pub fn fuzzy_positions(&mut self, text: &str) -> Vec<usize> {
        let mut indices = Vec::new();
        self.pattern.indices(
            Utf32Str::new(text, &mut self.buf),
            &mut self.matcher,
            &mut indices,
        );
        let mut positions: Vec<usize> = indices.into_iter().map(|index| index as usize).collect();
        positions.sort_unstable();
        positions.dedup();
        positions
    }

    /// Char indices to highlight for a reader: each typed word where it first occurs when the
    /// text holds every word, otherwise nucleo's scattered letters.
    pub fn positions(&mut self, text: &str) -> Vec<usize> {
        let folded = self.folded(text);
        // Lowercasing can change byte lengths; only map words when it did not.
        if folded.len() == text.len() {
            let mut positions = Vec::new();
            let mut all = true;
            for word in &self.words {
                match folded.find(word.as_str()) {
                    Some(at)
                        if text.is_char_boundary(at) && text.is_char_boundary(at + word.len()) =>
                    {
                        let first = text[..at].chars().count();
                        positions.extend(first..first + text[at..at + word.len()].chars().count());
                    }
                    _ => {
                        all = false;
                        break;
                    }
                }
            }
            if all && !positions.is_empty() {
                positions.sort_unstable();
                positions.dedup();
                return positions;
            }
        }
        self.fuzzy_positions(text)
    }
}

/// Char indices to byte ranges in `text`, joining neighbours into one range.
pub fn char_positions_to_byte_ranges(
    text: &str,
    positions: &[usize],
) -> Vec<std::ops::Range<usize>> {
    let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
    let mut wanted = positions.iter().copied().peekable();
    for (index, (at, character)) in text.char_indices().enumerate() {
        while wanted.peek().is_some_and(|&next| next < index) {
            wanted.next();
        }
        if wanted.peek() != Some(&index) {
            continue;
        }
        let end = at + character.len_utf8();
        match ranges.last_mut() {
            Some(last) if last.end == at => last.end = end,
            _ => ranges.push(at..end),
        }
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_rank_v1_whole_words_rank_first_and_smart_case() {
        let mut ranker = FuzzyRankerV1::new("needle");
        let whole = ranker.rank("prefix needle here").unwrap();
        let scattered = ranker.rank("n e e d l e").unwrap();
        assert!(whole.whole && !scattered.whole);
        assert_eq!(whole.better(&scattered), std::cmp::Ordering::Less);
        assert!(ranker.rank("nothing").is_none());
        let mut upper = FuzzyRankerV1::new("Needle");
        assert!(upper.rank("needle").is_none());
        assert!(upper.rank("Needle").is_some());
        assert!(FuzzyRankerV1::new("  ").rank("x").is_none());
    }

    #[test]
    fn fuzzy_rank_v1_positions_prefer_words_and_map_to_bytes() {
        let mut ranker = FuzzyRankerV1::new("working needle");
        let text = "β working the needle";
        let positions = ranker.positions(text);
        assert_eq!(positions, [2, 3, 4, 5, 6, 7, 8, 14, 15, 16, 17, 18, 19]);
        let ranges = char_positions_to_byte_ranges(text, &positions);
        assert_eq!(&text[ranges[0].clone()], "working");
        assert_eq!(&text[ranges[1].clone()], "needle");
        let mut scattered = FuzzyRankerV1::new("cmpl");
        let line = "compiling";
        let picked: String = scattered
            .positions(line)
            .iter()
            .map(|&index| line.chars().nth(index).unwrap())
            .collect();
        assert_eq!(picked, "cmpl");
    }
}
