//! Undeclared source-only quality fixture. Judgments are fixed before execution;
//! registration, measured results and real-terminal quality journeys are pending.
//! This small lexical corpus is not a performance or semantic-search benchmark.
use crate::{
    git::RawPath,
    providers::{gh::Actor, runner::CancellationToken},
    search::{
        corpus::Corpus,
        index::{RankedIndex, RankedLimits},
        model::{
            Coverage, Destination, DiscussionContext, DiscussionKind, Document, DocumentKey,
            Metadata, ResultKind, SourceKind,
        },
        scan::SearchPage,
        Query, SearchMode,
    },
    storage::{Cache, Scope},
};
use std::{collections::BTreeMap, path::Path};

const SOURCE: &str = "fixed-relevance-corpus-v1";
const AUTH_IMPL: &str = "auth-implementation";
const AUTH_CALLER: &str = "auth-caller";
const AUTH_NOTES: &str = "auth-notes";
const SNAKE_IMPL: &str = "snake-implementation";
const HTTP_IMPL: &str = "http-implementation";
const PR: &str = "PR_auth_design";
const COMMENT: &str = "IC_auth_explanation";
const FOREIGN: &str = "other-repository-implementation";
const MIXED_CASE: &str = "IC_canonical_repository_case";

fn scope() -> Scope {
    Scope::github("github.invalid", Some(42)).unwrap()
}

fn file(id: &str, repository: &str, path: &str, body: &str, selected: bool) -> Document {
    let path = RawPath::new(path.as_bytes().to_vec());
    Document {
        key: DocumentKey {
            scope: scope(),
            repository: repository.into(),
            revision: "working".into(),
            kind: ResultKind::File,
            path: Some(path.clone()),
            resource: id.into(),
        },
        source: SourceKind::Worktree,
        title: path.display(),
        body: body.into(),
        metadata: Metadata {
            language: Some("Rust".into()),
            repository_sets: if selected {
                vec!["selected".into()]
            } else {
                vec![]
            },
            complete: true,
            ..Default::default()
        },
        destination: Destination::LocalFile {
            directory: Path::new("/quality-fixture-not-opened").into(),
            path,
            revision: None,
            line: 1,
            column: 1,
        },
    }
}

fn discussion(id: &str, repository: &str, title: &str, body: &str, comment: bool) -> Document {
    let kind = if comment {
        DiscussionKind::ConversationComment
    } else {
        DiscussionKind::PullRequestBody
    };
    Document {
        key: DocumentKey {
            scope: scope(),
            repository: repository.into(),
            revision: "discussion-resource-v1".into(),
            kind: if comment {
                ResultKind::Comment
            } else {
                ResultKind::PullRequest
            },
            path: None,
            resource: id.into(),
        },
        source: SourceKind::Collaboration,
        title: title.into(),
        body: body.into(),
        metadata: Metadata {
            actors: vec![Actor {
                login: "reviewer".into(),
                actor_type: "User".into(),
                ..Default::default()
            }],
            labels: vec!["security".into()],
            repository_sets: vec!["selected".into()],
            fetched_at: 1_000,
            complete: true,
            discussion: Some(DiscussionContext {
                kind,
                repository_id: 101,
                parent_node: PR.into(),
                parent_number: 10,
                resource_node: id.into(),
                resource_numeric_id: Some(if !comment {
                    700
                } else if id == MIXED_CASE {
                    502
                } else {
                    501
                }),
                thread_id: None,
                path: None,
                commit_sha: None,
                side: None,
                line: None,
                start_line: None,
                outdated: false,
                resolved: None,
                url: None,
            }),
            ..Default::default()
        },
        destination: Destination::PullRequest {
            host: scope().host,
            repository: repository.into(),
            number: 10,
            comment_id: comment.then(|| id.into()),
            thread_id: None,
        },
    }
}

fn corpus() -> Corpus {
    let documents = vec![
        file(
            AUTH_IMPL,
            "org/app",
            "src/auth/AuthBoundary.rs",
            "Header without query terms\n你好 AuthBoundary keeps safe state\n",
            true,
        ),
        file(
            AUTH_CALLER,
            "org/app",
            "src/http/router.rs",
            "Dispatch invokes AuthBoundary once.",
            true,
        ),
        file(
            AUTH_NOTES,
            "org/app",
            "docs/security.md",
            "AuthBoundary overview and migration notes. Refresh token cache after rotation.",
            true,
        ),
        file(
            SNAKE_IMPL,
            "org/app",
            "src/auth/refresh_token_cache.rs",
            "fn refresh_token_cache() { rotate(); }",
            true,
        ),
        file(
            HTTP_IMPL,
            "org/app",
            "src/http/HTTPServer2.rs",
            "HTTPServer2 delivers bounded responses.",
            true,
        ),
        discussion(
            PR,
            "org/app",
            "Review AuthBoundary",
            "AuthBoundary review summary and the design decision.",
            false,
        ),
        discussion(
            COMMENT,
            "org/app",
            "AuthBoundary reviewer explanation",
            "Use state fences to prevent stale private search.",
            true,
        ),
        file(
            "phrase-decoy",
            "org/app",
            "docs/state.md",
            "search stale distant private: these words are not the requested phrase.",
            true,
        ),
        file(
            FOREIGN,
            "org/other",
            "src/auth/AuthBoundary.rs",
            "AuthBoundary keeps safe state",
            false,
        ),
        discussion(
            MIXED_CASE,
            "Org/App",
            "Canonical repository witness",
            "CanonicalRepoCaseMarker belongs to this same GitHub repository.",
            true,
        ),
    ];
    Corpus {
        scope: scope(),
        coverage: Coverage {
            documents: documents.len(),
            repositories: vec!["org/app".into(), "org/other".into()],
            complete: true,
            deletion_authoritative: true,
            ..Default::default()
        },
        documents,
    }
}

struct Fixture {
    _directory: tempfile::TempDir,
    _cache: Cache,
    index: RankedIndex,
    versions: BTreeMap<String, String>,
}
impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut cache = Cache::memory().unwrap();
        let cancel = CancellationToken::default();
        let mut index = RankedIndex::open(
            directory.path(),
            scope(),
            &cache,
            RankedLimits::default(),
            &cancel,
        )
        .unwrap();
        let corpus = corpus();
        assert_eq!(corpus.documents.len(), 10);
        let versions = corpus
            .documents
            .iter()
            .map(|document| {
                (
                    document.key.stable_id(),
                    document.projection_checksum().unwrap(),
                )
            })
            .collect();
        index
            .publish(&mut cache, SOURCE, 1, &corpus, &cancel)
            .unwrap();
        Self {
            _directory: directory,
            _cache: cache,
            index,
            versions,
        }
    }
    fn search(&self, text: &str) -> SearchPage {
        self.index
            .search_source_current(
                SOURCE,
                &Query::parse(text, SearchMode::Ranked).unwrap(),
                &self.versions,
                &CancellationToken::default(),
            )
            .unwrap()
    }
}

struct Judgment {
    query: &'static str,
    primary: &'static str,
    graded: &'static [(&'static str, u8)],
}

// Grades describe product usefulness, not a captured engine ranking: 3 is the
// primary answer, 1 is useful supporting context, and omitted documents score0.
const JUDGMENTS: &[Judgment] = &[
    Judgment {
        query: "type:file repo:org/app AuthBoundary",
        primary: AUTH_IMPL,
        graded: &[(AUTH_IMPL, 3), (AUTH_CALLER, 1), (AUTH_NOTES, 1)],
    },
    Judgment {
        query: "repo:org/app type:file path:src/auth AuthBoundary",
        primary: AUTH_IMPL,
        graded: &[(AUTH_IMPL, 3)],
    },
    Judgment {
        query: "type:file set:selected AuthBoundary",
        primary: AUTH_IMPL,
        graded: &[(AUTH_IMPL, 3), (AUTH_CALLER, 1), (AUTH_NOTES, 1)],
    },
    Judgment {
        query: "type:file refresh_token_cache",
        primary: SNAKE_IMPL,
        graded: &[(SNAKE_IMPL, 3), (AUTH_NOTES, 1)],
    },
    Judgment {
        query: "type:file HTTPServer2",
        primary: HTTP_IMPL,
        graded: &[(HTTP_IMPL, 3)],
    },
    Judgment {
        query: "repo:org/app \"keeps safe state\"",
        primary: AUTH_IMPL,
        graded: &[(AUTH_IMPL, 3)],
    },
    Judgment {
        query: "type:pr AuthBoundary",
        primary: PR,
        graded: &[(PR, 3)],
    },
    Judgment {
        query: "type:comment author:reviewer \"stale private search\"",
        primary: COMMENT,
        graded: &[(COMMENT, 3)],
    },
    Judgment {
        query: "type:file repo:org/other AuthBoundary",
        primary: FOREIGN,
        graded: &[(FOREIGN, 3)],
    },
    Judgment {
        query: "type:comment CanonicalRepoCaseMarker",
        primary: MIXED_CASE,
        graded: &[(MIXED_CASE, 3)],
    },
];

fn gain(grade: u8) -> f64 {
    f64::from((1u32 << grade) - 1)
}
fn ndcg_at_three(ids: &[&str], judgment: &Judgment) -> f64 {
    let grade = |id: &str| {
        judgment
            .graded
            .iter()
            .find(|(candidate, _)| *candidate == id)
            .map_or(0, |(_, grade)| *grade)
    };
    let dcg = ids
        .iter()
        .take(3)
        .enumerate()
        .map(|(rank, id)| gain(grade(id)) / ((rank + 2) as f64).log2())
        .sum::<f64>();
    let mut ideal = judgment
        .graded
        .iter()
        .map(|(_, grade)| *grade)
        .collect::<Vec<_>>();
    ideal.sort_unstable_by(|left, right| right.cmp(left));
    let ideal = ideal
        .into_iter()
        .take(3)
        .enumerate()
        .map(|(rank, grade)| gain(grade) / ((rank + 2) as f64).log2())
        .sum::<f64>();
    dcg / ideal
}

#[test]
fn search_relevance_quality_fixed_labeled_corpus_top_answers_mrr_and_ndcg() {
    let fixture = Fixture::new();
    let mut reciprocal_sum = 0.0;
    for judgment in JUDGMENTS {
        let page = fixture.search(judgment.query);
        assert!(
            page.coverage.complete && !page.coverage.cancelled,
            "Fixed finite corpus must be fully covered: {}",
            judgment.query
        );
        let ids = page
            .hits
            .iter()
            .map(|hit| hit.document.key.resource.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            ids.first().copied(),
            Some(judgment.primary),
            "Independent primary-answer judgment: {}",
            judgment.query
        );
        let primary_rank = ids.iter().position(|id| *id == judgment.primary).unwrap() + 1;
        reciprocal_sum += 1.0 / primary_rank as f64;
        assert!(
            ndcg_at_three(&ids, judgment) >= 0.90,
            "nDCG@3 below fixed0.90 semantic-judgment floor: {}: {ids:?}",
            judgment.query
        );
        assert!(page
            .hits
            .iter()
            .all(|hit| hit.document.matches(&page.query)));
    }
    assert_eq!(
        reciprocal_sum / JUDGMENTS.len() as f64,
        1.0,
        "MRR and success@1 must remain1.0 for the declared primary answers"
    );
    assert!(
        fixture.search("\"search stale private\"").hits.is_empty(),
        "Disconnected word co-occurrence must not satisfy a quoted phrase"
    );
}

#[test]
fn search_relevance_quality_snippet_fields_unicode_and_exact_discussion_destination_agree() {
    let fixture = Fixture::new();
    let page = fixture.search("type:file repo:org/app AuthBoundary");
    let hit = &page.hits[0];
    assert_eq!(hit.document.key.resource, AUTH_IMPL);
    assert_eq!((hit.line, hit.column), (2, 4));
    assert_eq!(hit.snippet, "你好 AuthBoundary keeps safe state");
    assert_eq!(
        hit.matched_fields,
        [
            "title lexical tokens",
            "path lexical tokens",
            "body lexical tokens"
        ]
    );
    assert_eq!(hit.highlights.len(), 1);
    let range = &hit.highlights[0];
    assert!(hit.snippet.is_char_boundary(range.start) && hit.snippet.is_char_boundary(range.end));
    assert_eq!(&hit.snippet[range.clone()], "AuthBoundary");
    assert!(matches!(
        hit.destination(),
        Destination::LocalFile {
            revision: None,
            line: 2,
            column: 4,
            ..
        }
    ));
    let page = fixture.search("type:comment author:reviewer \"stale private search\"");
    let hit = &page.hits[0];
    assert_eq!(hit.document.key.resource, COMMENT);
    assert_eq!(hit.document.source, SourceKind::Collaboration);
    assert_eq!(hit.matched_fields, ["body lexical tokens"]);
    assert_eq!(
        hit.document
            .metadata
            .discussion
            .as_ref()
            .unwrap()
            .resource_node,
        COMMENT
    );
    assert!(
        matches!(hit.destination(),Destination::PullRequest {number:10,comment_id:Some(ref id),thread_id:None,..} if id == COMMENT)
    );
    assert_eq!(
        fixture.versions[&hit.document.key.stable_id()],
        hit.document.projection_checksum().unwrap()
    );
}

#[test]
fn search_relevance_quality_github_repo_filter_uses_canonical_case_insensitive_identity() {
    let fixture = Fixture::new();
    let unfiltered = fixture.search("type:comment CanonicalRepoCaseMarker");
    assert_eq!(unfiltered.hits.len(), 1);
    assert_eq!(unfiltered.hits[0].document.key.repository, "Org/App");
    let canonical = fixture.search("repo:org/app type:comment CanonicalRepoCaseMarker");
    assert_eq!(canonical.hits.len(),1,"A normalized GitHub repository filter must not discard an identically scoped/identified canonical mixed-case repository");
    assert_eq!(
        canonical.hits[0].document.key,
        unfiltered.hits[0].document.key
    );
    assert_eq!(
        canonical.hits[0].destination(),
        unfiltered.hits[0].destination()
    );
    for filter in ["repo:ORG/APP", "org:org", "org:ORG"] {
        let page = fixture.search(&format!("{filter} type:comment CanonicalRepoCaseMarker"));
        assert_eq!(page.hits.len(), 1);
        assert_eq!(page.hits[0].document.key, unfiltered.hits[0].document.key);
        assert_eq!(page.hits[0].destination(), unfiltered.hits[0].destination());
    }
}

#[test]
fn search_relevance_quality_repository_case_does_not_normalize_local_scope_paths_sets_or_principals(
) {
    let github = discussion(
        MIXED_CASE,
        "Org/App",
        "Canonical repository witness",
        "CanonicalRepoCaseMarker",
        true,
    );
    let key = github.key.clone();
    let destination = github.destination.clone();
    for mode in [
        SearchMode::Exact,
        SearchMode::Regex,
        SearchMode::Ranked,
        SearchMode::GitHub,
    ] {
        for filter in ["repo:org/app", "repo:ORG/APP", "org:org", "org:ORG"] {
            assert!(github.matches(&Query::parse(filter, mode).unwrap()));
        }
        assert!(!github.matches(&Query::parse("repo:org/other", mode).unwrap()));
        assert!(!github.matches(&Query::parse("org:other", mode).unwrap()));
    }
    assert_eq!(github.key, key);
    assert_eq!(github.destination, destination);
    let mut local = file(
        "raw-local-boundary",
        "Org/App",
        "src/AuthBoundary.rs",
        "AuthBoundary",
        true,
    );
    local.key.scope = Scope::new("local", "workspace").unwrap();
    assert!(local.matches(&Query::parse("repo:Org/App org:Org", SearchMode::Ranked).unwrap()));
    assert!(!local.matches(&Query::parse("repo:org/app", SearchMode::Ranked).unwrap()));
    assert!(!local.matches(&Query::parse("org:org", SearchMode::Ranked).unwrap()));
    assert!(!local.matches(&Query::parse("path:src/authboundary.rs", SearchMode::Ranked).unwrap()));
    assert!(local.matches(
        &Query::parse("path:src/AuthBoundary.rs set:selected", SearchMode::Ranked).unwrap()
    ));
    assert!(!local.matches(&Query::parse("set:Selected", SearchMode::Ranked).unwrap()));
    let mut legacy = github.clone();
    legacy.key.scope = Scope::new("github.invalid", "reusable-login").unwrap();
    assert!(!legacy.matches(&Query::parse("repo:org/app", SearchMode::Ranked).unwrap()));
    assert!(legacy.matches(&Query::parse("repo:Org/App", SearchMode::Ranked).unwrap()));
    let mut missing_identity = github.clone();
    missing_identity
        .metadata
        .discussion
        .as_mut()
        .unwrap()
        .repository_id = 0;
    assert!(!missing_identity.matches(&Query::parse("repo:org/app", SearchMode::Ranked).unwrap()));
    let mut missing_context = github.clone();
    missing_context.metadata.discussion = None;
    assert!(!missing_context.matches(&Query::parse("repo:org/app", SearchMode::Ranked).unwrap()));
    let mut mismatched_parent = github.clone();
    mismatched_parent
        .metadata
        .discussion
        .as_mut()
        .unwrap()
        .parent_number = 99;
    assert!(!mismatched_parent.matches(&Query::parse("repo:org/app", SearchMode::Ranked).unwrap()));
    let mut remote_fragment = github.clone();
    remote_fragment.source = SourceKind::GitHub;
    assert!(!remote_fragment.matches(&Query::parse("repo:org/app", SearchMode::Ranked).unwrap()));
    let mut unmapped = file(
        "unmapped-local-boundary",
        "local/current",
        "src/AuthBoundary.rs",
        "AuthBoundary",
        true,
    );
    assert!(!unmapped.matches(&Query::parse("repo:LOCAL/CURRENT", SearchMode::Ranked).unwrap()));
    unmapped.key.scope = scope();
    assert!(!unmapped.matches(&Query::parse("repo:LOCAL/CURRENT", SearchMode::Ranked).unwrap()));
    let other = Scope::github("github.invalid", Some(43)).unwrap();
    assert!(github.validate(&other).is_err());
    let mut reassigned = github.clone();
    reassigned.key.scope = other;
    assert_ne!(reassigned.key.stable_id(), key.stable_id());
}
