//! TASK-122: the one fuzzy finder every list filter and picker shares.
//!
//! It is the TASK-116 log finder's ranking, lifted out so the Actions log
//! finder, the command palette, the Local operations target pickers, the saved
//! path jump, the changed-files picker and History all match, rank and
//! highlight the same way: nucleo fuzzy with smart case, entries that contain
//! every typed word as written before scattered matches, then the better
//! score, then the original order. Pure and bounded: no GitHub, SQL or Git.
use nucleo_matcher::{
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
    Matcher, Utf32Str,
};
use ratatui::{
    style::{Modifier, Style},
    text::Span,
};

/// One compiled query: the nucleo pattern plus the typed words used to put
/// contiguous matches first.
pub struct FuzzyQueryV1 {
    pattern: Pattern,
    matcher: Matcher,
    buf: Vec<char>,
    words: Vec<String>,
    sensitive: bool,
    empty: bool,
}

impl FuzzyQueryV1 {
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
            words: query
                .split_whitespace()
                .map(|w| {
                    if sensitive {
                        w.to_owned()
                    } else {
                        w.to_lowercase()
                    }
                })
                .collect(),
            sensitive,
            empty: query.trim().is_empty(),
        }
    }
    /// Nothing typed: callers show their whole list (or ask to type).
    pub fn is_empty(&self) -> bool {
        self.empty
    }
    /// The fuzzy score, when `text` matches.
    pub fn score(&mut self, text: &str) -> Option<u32> {
        self.pattern
            .score(Utf32Str::new(text, &mut self.buf), &mut self.matcher)
    }
    /// Whether `text` contains every typed word as written (smart case).
    pub fn whole_words(&self, text: &str) -> bool {
        if self.sensitive {
            self.words.iter().all(|w| text.contains(w.as_str()))
        } else {
            let folded = text.to_lowercase();
            self.words.iter().all(|w| folded.contains(w.as_str()))
        }
    }
    /// The ranking key of `text`: (every word contiguous, fuzzy score).
    pub fn rank_key(&mut self, text: &str) -> Option<(bool, u32)> {
        let score = self.score(text)?;
        Some((self.whole_words(text), score))
    }
    /// Sorted, unique char indices of the matched characters of `text`.
    pub fn positions(&mut self, text: &str) -> Option<Vec<usize>> {
        if self.empty {
            return None;
        }
        let mut indices = Vec::new();
        self.pattern.indices(
            Utf32Str::new(text, &mut self.buf),
            &mut self.matcher,
            &mut indices,
        )?;
        let mut positions: Vec<usize> = indices.into_iter().map(|i| i as usize).collect();
        positions.sort_unstable();
        positions.dedup();
        Some(positions)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuzzyHitV1 {
    /// Index of the entry in the ranked input.
    pub index: usize,
    pub score: u32,
    /// Sorted, unique char indices of the matched characters.
    pub positions: Vec<usize>,
}

/// Ranks `texts` best first and returns (every match, the first `cap` hits).
/// `check(index)` runs before each entry so callers can enforce deadlines or
/// cancellation. An empty query matches nothing.
pub fn rank_v1<'a, I, F>(
    query: &str,
    texts: I,
    cap: usize,
    mut check: F,
) -> Result<(usize, Vec<FuzzyHitV1>), String>
where
    I: IntoIterator<Item = &'a str>,
    F: FnMut(usize) -> Result<(), String>,
{
    let mut finder = FuzzyQueryV1::new(query);
    if finder.is_empty() {
        return Ok((0, vec![]));
    }
    let texts: Vec<&str> = texts.into_iter().collect();
    let mut scored = Vec::new();
    for (index, text) in texts.iter().enumerate() {
        check(index)?;
        if let Some((whole, score)) = finder.rank_key(text) {
            scored.push((index, whole, score));
        }
    }
    let total = scored.len();
    scored.sort_by(|(a, wa, sa), (b, wb, sb)| wb.cmp(wa).then(sb.cmp(sa)).then(a.cmp(b)));
    scored.truncate(cap);
    let hits = scored
        .into_iter()
        .map(|(index, _, score)| FuzzyHitV1 {
            index,
            score,
            positions: finder.positions(texts[index]).unwrap_or_default(),
        })
        .collect();
    Ok((total, hits))
}

/// Word-bounded fuzzy matching for lists that keep their own order (History):
/// every typed word must fuzzy-match inside one word of `text`, so `cfg`
/// finds "config" but letters scattered across a sentence do not count.
/// Returns the matched char indices.
pub fn word_positions_v1(query: &str, text: &str) -> Option<Vec<usize>> {
    let mut out = Vec::new();
    let mut any = false;
    for word in query.split_whitespace() {
        any = true;
        let mut finder = FuzzyQueryV1::new(word);
        let mut found = None;
        let mut start = 0usize;
        for token in text.split(char::is_whitespace) {
            let len = token.chars().count();
            if !token.is_empty() {
                if let Some(positions) = finder.positions(token) {
                    found = Some(positions.into_iter().map(|p| p + start).collect::<Vec<_>>());
                    break;
                }
            }
            start += len + 1;
        }
        out.extend(found?);
    }
    if !any {
        return None;
    }
    out.sort_unstable();
    out.dedup();
    Some(out)
}

/// The highlight every finder gives matched characters.
pub fn mark_style_v1(theme: crate::theme::Theme) -> Style {
    theme
        .style(crate::theme::SemanticToken::Accent)
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
}

/// Spans of `text` from char `start`, at most `cells` terminal cells wide, with
/// the chars at `positions` patched with `mark`. Control characters print as
/// spaces so a row never moves the cursor.
pub fn highlight_spans_v1(
    text: &str,
    positions: &[usize],
    start: usize,
    cells: usize,
    base: Style,
    mark: Style,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut run = String::new();
    let mut run_marked = false;
    let mut used = 0usize;
    let mut wanted = positions.iter().peekable();
    for (i, ch) in text.chars().enumerate().skip(start) {
        let ch = if ch.is_control() { ' ' } else { ch };
        let mut utf8 = [0u8; 4];
        let width = crate::editor::grapheme_width(ch.encode_utf8(&mut utf8));
        if used + width > cells {
            break;
        }
        used += width;
        while wanted.peek().is_some_and(|p| **p < i) {
            wanted.next();
        }
        let marked = wanted.peek().is_some_and(|p| **p == i);
        if marked != run_marked && !run.is_empty() {
            spans.push(Span::styled(
                std::mem::take(&mut run),
                if run_marked { base.patch(mark) } else { base },
            ));
        }
        run_marked = marked;
        run.push(ch);
    }
    if !run.is_empty() {
        spans.push(Span::styled(
            run,
            if run_marked { base.patch(mark) } else { base },
        ));
    }
    spans
}

/// "N of M" for the count every finder shows beside its filter.
pub fn count_label_v1(matching: usize, all: usize) -> String {
    format!("{matching} of {all}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_finder_v1_ranks_contiguous_words_first_then_score_then_order() {
        let texts = [
            "e r r o r scattered",
            "error[E0425]: missing",
            "nothing here",
            "##[error] exit",
        ];
        let (total, hits) = rank_v1("error", texts, 10, |_| Ok(())).unwrap();
        assert_eq!(total, 3);
        let order: Vec<usize> = hits.iter().map(|h| h.index).collect();
        assert_eq!(*order.last().unwrap(), 0, "{order:?}");
        let (_, capped) = rank_v1("error", texts, 1, |_| Ok(())).unwrap();
        assert_eq!(capped.len(), 1);
        assert_eq!(rank_v1("  ", texts, 10, |_| Ok(())).unwrap(), (0, vec![]));
        assert!(rank_v1("error", texts, 10, |_| Err("stop".into())).is_err());
    }

    #[test]
    fn fuzzy_finder_v1_positions_are_chars_and_smart_case() {
        let mut finder = FuzzyQueryV1::new("cmpl");
        let text = "界 compiling";
        let picked: String = finder
            .positions(text)
            .unwrap()
            .iter()
            .map(|&i| text.chars().nth(i).unwrap())
            .collect();
        assert_eq!(picked, "cmpl");
        let mut upper = FuzzyQueryV1::new("Main");
        assert!(upper.score("main").is_none());
        assert!(upper.score("Main branch").is_some());
        let mut lower = FuzzyQueryV1::new("main");
        assert!(lower.score("MAIN").is_some());
    }

    #[test]
    fn fuzzy_finder_v1_word_positions_stay_inside_one_word() {
        assert_eq!(
            word_positions_v1("cfg", "Parse the config header"),
            Some(vec![10, 13, 15])
        );
        // "parser" letters exist across "Parse the config header" but not in one word.
        assert_eq!(word_positions_v1("parser", "Parse the config header"), None);
        assert!(word_positions_v1("tune prsr", "Tune the parser defaults").is_some());
        assert_eq!(word_positions_v1("", "anything"), None);
    }

    #[test]
    fn fuzzy_finder_v1_highlight_spans_mark_matches_and_respect_width() {
        let base = Style::default();
        let mark = Style::default().add_modifier(Modifier::BOLD);
        let spans = highlight_spans_v1("feature/parser", &[8, 9], 0, 40, base, mark);
        let texts: Vec<&str> = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(texts, vec!["feature/", "pa", "rser"]);
        assert_eq!(spans[1].style, mark);
        let cut = highlight_spans_v1("a\tb界c", &[], 0, 4, base, mark);
        assert_eq!(cut[0].content, "a b");
        assert_eq!(count_label_v1(3, 40), "3 of 40");
    }
}
