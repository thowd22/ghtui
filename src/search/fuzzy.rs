//! Nucleo is only for navigation pickers; it never substitutes for a content search mode.
use super::QueryError;
use crate::providers::runner::CancellationToken;
use nucleo_matcher::{
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
    Matcher, Utf32Str,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerItem {
    pub id: String,
    pub label: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerHit {
    pub item: PickerItem,
    pub score: u32,
    /// Unicode character indexes, not bytes or terminal cells. Rendering maps these explicitly.
    pub indices: Vec<u32>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerPage {
    pub hits: Vec<PickerHit>,
    pub complete: bool,
    pub cancelled: bool,
}

pub fn pick(
    needle: &str,
    items: &[PickerItem],
    limit: usize,
    cancellation: &CancellationToken,
) -> Result<PickerPage, QueryError> {
    if needle.chars().count() > 128 || items.len() > 100_000 || limit == 0 || limit > 1000 {
        return Err(QueryError(
            "Picker limits:128 query characters,100000 candidates,1–1000 visible matches".into(),
        ));
    }
    if items.iter().any(|item| item.label.chars().count() > 4096) {
        return Err(QueryError(
            "Picker labels must be at most4096 characters".into(),
        ));
    }
    let pattern = Pattern::new(
        needle,
        CaseMatching::Smart,
        Normalization::Smart,
        AtomKind::Fuzzy,
    );
    let mut matcher = Matcher::new(nucleo_matcher::Config::DEFAULT);
    let mut buffer = Vec::new();
    let mut scores = Vec::new();
    let mut cancelled = false;
    for (index, item) in items.iter().enumerate() {
        if cancellation.is_cancelled() {
            cancelled = true;
            break;
        }
        if let Some(score) = pattern.score(Utf32Str::new(&item.label, &mut buffer), &mut matcher) {
            scores.push((index, score));
        }
    }
    scores.sort_by(|(a, ascore), (b, bscore)| {
        bscore
            .cmp(ascore)
            .then_with(|| (&items[*a].label, &items[*a].id).cmp(&(&items[*b].label, &items[*b].id)))
    });
    let complete = !cancelled && scores.len() <= limit;
    let hits = scores
        .into_iter()
        .take(limit)
        .filter_map(|(index, score)| {
            let mut indices = Vec::new();
            pattern.indices(
                Utf32Str::new(&items[index].label, &mut buffer),
                &mut matcher,
                &mut indices,
            )?;
            indices.sort_unstable();
            indices.dedup();
            Some(PickerHit {
                item: items[index].clone(),
                score,
                indices,
            })
        })
        .collect();
    Ok(PickerPage {
        hits,
        complete,
        cancelled,
    })
}
