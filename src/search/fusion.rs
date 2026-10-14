//! SOURCE-ONLY PREPARATION: not exported, compiled, verified or wired to the UI.
//!
//! This pure rank-list stage never authorizes a source. Its eventual worker caller
//! must validate principal/query/repository scope, current projection visibility,
//! canonical aliases and destinations BEFORE supplying entries, then recheck its
//! read proofs before showing counts/snippets or accepting an Open. Exact/Regex
//! stay local; GitHub mode retains provider order. Blended Ranked requires an
//! explicit remote opt-in and per-endpoint coverage/budgets. No implicit gh reads.
//!
//! RRF baseline: Cormack/Clarke/Buettcher SIGIR2009, k=60, using ordinal ranks
//! rather than incomparable engine scores. This is not a relevance-quality claim.
//! https://cormack.uwaterloo.ca/cormacksigir09-rrf.pdf
//!
//! Alias construction must distinguish opaque node identities from numeric REST
//! fallbacks. Never merge PR/issue parents by number/title, comments with parent
//! bodies, mutable files with saved files, or unresolved indexed blobs with an
//! inferred commit. Known-invalidated aliases cannot bypass cache fences through
//! remote fragments. Canonical current bodies win presentation, not authorization.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub mod blend;
pub mod ui;

const RRF_K: f64 = 60.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RankingSource {
    LocalLexical,
    GithubCode,
    GithubPullRequests,
    GithubIssues,
    GithubRepositories,
}
impl RankingSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::LocalLexical => "Local lexical",
            Self::GithubCode => "GitHub code",
            Self::GithubPullRequests => "GitHub PRs",
            Self::GithubIssues => "GitHub issues",
            Self::GithubRepositories => "GitHub repositories",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Presentation {
    RemoteFragment,
    AuthorizedCurrentDocument,
}

/// Keys/representatives are opaque identities prepared by a future validated
/// alias resolver, not URL/display-label-derived identities constructed here.
#[derive(Debug, Clone)]
pub struct RankEntry {
    pub alias: String,
    pub representative: String,
    pub presentation: Presentation,
}
#[derive(Debug, Clone)]
pub struct RankList {
    pub source: RankingSource,
    pub entries: Vec<RankEntry>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct FusedEntry {
    pub alias: String,
    pub representative: String,
    pub presentation: Presentation,
    pub score: f64,
    pub contributions: Vec<(RankingSource, usize)>,
}

pub fn fuse(lists: &[RankList], max_results: usize) -> Result<Vec<FusedEntry>, &'static str> {
    if !(1..=5000).contains(&max_results) || lists.len() > 5 {
        return Err("Fusion requires bounded result and source counts");
    }
    let mut sources = BTreeSet::new();
    let mut ordered: Vec<_> = lists.iter().collect();
    ordered.sort_by_key(|list| list.source);
    let mut results: BTreeMap<String, FusedEntry> = BTreeMap::new();
    for list in ordered {
        if !sources.insert(list.source) || list.entries.len() > 5000 {
            return Err("Duplicate source/page votes or oversized rank list");
        }
        let mut seen = BTreeSet::new();
        for (index, candidate) in list.entries.iter().enumerate() {
            if candidate.alias.is_empty()
                || candidate.alias.len() > 1024
                || candidate.representative.is_empty()
                || candidate.representative.len() > 1024
            {
                return Err("Missing or oversized prevalidated fusion identity");
            }
            let entry = results
                .entry(candidate.alias.clone())
                .or_insert_with(|| FusedEntry {
                    alias: candidate.alias.clone(),
                    representative: candidate.representative.clone(),
                    presentation: candidate.presentation,
                    score: 0.0,
                    contributions: vec![],
                });
            if candidate.presentation > entry.presentation
                || candidate.presentation == entry.presentation
                    && candidate.representative < entry.representative
            {
                entry.representative.clone_from(&candidate.representative);
                entry.presentation = candidate.presentation;
            }
            if seen.insert(&candidate.alias) {
                let rank = index + 1;
                entry.score += 1.0 / (RRF_K + rank as f64);
                entry.contributions.push((list.source, rank));
            }
        }
    }
    let mut result: Vec<_> = results.into_values().collect();
    result.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.alias.cmp(&right.alias))
    });
    result.truncate(max_results);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn list(source: RankingSource, aliases: &[&str]) -> RankList {
        RankList {
            source,
            entries: aliases
                .iter()
                .map(|id| RankEntry {
                    alias: (*id).into(),
                    representative: (*id).into(),
                    presentation: Presentation::RemoteFragment,
                })
                .collect(),
        }
    }

    #[test]
    fn collaboration_index_fusion_cross_source_agreement_uses_ranks_not_raw_scores() {
        let lists = [
            list(RankingSource::LocalLexical, &["local-first", "agreed"]),
            list(
                RankingSource::GithubPullRequests,
                &["remote-first", "agreed"],
            ),
        ];
        let result = fuse(&lists, 10).unwrap();
        assert_eq!(result[0].alias, "agreed");
        assert_eq!(result[0].score, 2.0 / 62.0);
        assert_eq!(
            result[0].contributions,
            [
                (RankingSource::LocalLexical, 2),
                (RankingSource::GithubPullRequests, 2)
            ]
        );
    }
    #[test]
    fn collaboration_index_fusion_same_source_duplicate_cannot_inflate_vote() {
        let result = fuse(&[list(RankingSource::LocalLexical, &["a", "a", "b"])], 10).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].score, 1.0 / 61.0);
        assert_eq!(result[0].contributions.len(), 1);
        assert_eq!(result[1].contributions[0].1, 3);
    }
    #[test]
    fn collaboration_index_fusion_provider_page_splits_cannot_supply_extra_votes() {
        let source = list(RankingSource::GithubIssues, &["a"]);
        assert!(fuse(&[source.clone(), source], 10).is_err());
    }
    #[test]
    fn collaboration_index_fusion_order_and_ties_are_deterministic_with_current_body_presentation()
    {
        let local = list(RankingSource::LocalLexical, &["a", "b"]);
        let mut remote = list(RankingSource::GithubPullRequests, &["b", "a"]);
        remote.entries[1].presentation = Presentation::AuthorizedCurrentDocument;
        remote.entries[1].representative = "current-projection-a".into();
        let first = fuse(&[local.clone(), remote.clone()], 10).unwrap();
        assert_eq!(first, fuse(&[remote, local], 10).unwrap());
        assert_eq!(first[0].alias, "a");
        assert_eq!(first[0].representative, "current-projection-a");
    }
    #[test]
    fn collaboration_index_fusion_empty_missing_and_bounded_inputs_do_not_fabricate_hits() {
        assert!(fuse(&[], 1).unwrap().is_empty());
        assert!(fuse(&[], 0).is_err());
        assert!(fuse(&[], 5001).is_err());
        assert!(fuse(&[list(RankingSource::LocalLexical, &[""])], 1).is_err());
        assert_eq!(
            fuse(&[list(RankingSource::LocalLexical, &["a", "b"])], 1)
                .unwrap()
                .len(),
            1
        );
    }
}
