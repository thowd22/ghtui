//! Typed GitHub discovery, preserving provider order and fragment-only provenance.
use super::{
    model::{
        valid_sha, Coverage, Destination, Document, DocumentKey, Metadata, ResultKind, SearchHit,
        SourceKind,
    },
    scan::SearchPage,
    Query, QueryError, SearchMode,
};
use crate::{
    git::RawPath,
    providers::gh::{HostIdentity, RemoteSearchItem, RemoteSearchKind, RemoteSearchResult},
    storage::Scope,
};
use std::{collections::BTreeMap, sync::Arc};

pub fn kind(query: &Query) -> Result<RemoteSearchKind, QueryError> {
    if query.mode != SearchMode::GitHub {
        return Err(QueryError(
            "Choose explicit GitHub mode for remote discovery".into(),
        ));
    }
    let kinds = query.filters.get("type").map(Vec::as_slice).unwrap_or(&[]);
    if kinds.len() > 1 {
        return Err(QueryError(
            "Choose one GitHub result type per search; provider ranks are not interchangeable"
                .into(),
        ));
    }
    match kinds.first().map(String::as_str).unwrap_or("file") {
        "file" => Ok(RemoteSearchKind::Code), "pr" => Ok(RemoteSearchKind::PullRequests),
        "issue" => Ok(RemoteSearchKind::Issues), "repository" => Ok(RemoteSearchKind::Repositories),
        _ => Err(QueryError("This remote endpoint cannot identify an exact comment/commit/run/workflow result; choose a hydrated local corpus".into())),
    }
}

pub fn verify_scope(scope: &Scope, identity: &HostIdentity) -> Result<(), QueryError> {
    let verified = Scope::github(&identity.host, identity.account_id)
        .map_err(|error| QueryError(error.to_string()))?;
    if scope != &verified || identity.login.trim().is_empty() {
        return Err(QueryError("Remote search host/account differs from the verified visible scope; prior private results remain hidden".into()));
    }
    Ok(())
}

pub fn prepare_page(
    scope: &Scope,
    query: &Query,
    result: RemoteSearchResult,
    max_hits: usize,
) -> Result<(SearchPage, BTreeMap<String, RemoteSearchItem>), QueryError> {
    verify_scope(scope, &result.identity)?;
    let expected = kind(query)?;
    if max_hits == 0 {
        return Err(QueryError("Remote result budget must be positive".into()));
    }
    let mut coverage = Coverage {
        complete: result.coverage.complete,
        documents: result.items.len().min(max_hits),
        reasons: result.coverage.reasons,
        ..Coverage::default()
    };
    coverage.reasons.extend(result.provider_notes);
    coverage.reasons.push(format!(
        "GitHub loaded {} of {} reported matches; provider best-match order, not local BM25 scores",
        result.coverage.loaded, result.coverage.total_matches
    ));
    if result.items.len() > max_hits {
        coverage.complete = false;
        coverage.reasons.push(format!(
            "Visible remote discovery reached its {max_hits}-result budget"
        ));
    }
    let mut page = SearchPage {
        query: query.clone(),
        hits: Vec::new(),
        coverage,
    };
    let mut items = BTreeMap::new();
    for (rank, item) in result.items.into_iter().take(max_hits).enumerate() {
        if item.kind != expected || item.host != scope.host {
            return Err(QueryError(
                "Remote provider returned a different result type or host".into(),
            ));
        }
        let (result_kind, revision, path, destination) = match item.kind {
            RemoteSearchKind::Code => {
                let path = RawPath::new(
                    item.path
                        .as_ref()
                        .ok_or_else(|| QueryError("Code candidate has no path".into()))?
                        .as_bytes()
                        .to_vec(),
                );
                let blob = item
                    .blob_sha
                    .as_ref()
                    .filter(|id| valid_sha(id))
                    .ok_or_else(|| QueryError("Code candidate has no valid indexed blob".into()))?
                    .clone();
                (
                    ResultKind::File,
                    format!("indexed-blob:{blob}"),
                    Some(path.clone()),
                    Destination::RemoteCodeCandidate {
                        host: item.host.clone(),
                        repository: item.repository.clone(),
                        path,
                        blob_sha: blob,
                    },
                )
            }
            RemoteSearchKind::PullRequests => (
                ResultKind::PullRequest,
                "remote-current-item".into(),
                None,
                Destination::PullRequest {
                    host: item.host.clone(),
                    repository: item.repository.clone(),
                    number: item.number.unwrap_or(0),
                    comment_id: None,
                    thread_id: None,
                },
            ),
            RemoteSearchKind::Issues => (
                ResultKind::Issue,
                "remote-current-item".into(),
                None,
                Destination::Issue {
                    host: item.host.clone(),
                    repository: item.repository.clone(),
                    number: item.number.unwrap_or(0),
                    comment_id: None,
                },
            ),
            RemoteSearchKind::Repositories => (
                ResultKind::Repository,
                "remote-current-item".into(),
                None,
                Destination::Repository {
                    host: item.host.clone(),
                    repository: item.repository.clone(),
                },
            ),
        };
        let title = path
            .as_ref()
            .map_or_else(|| item.title.clone(), RawPath::display);
        let body = if item.text_matches.is_empty() {
            bounded(&item.body, 64 * 1024)
        } else {
            bounded(
                &item
                    .text_matches
                    .iter()
                    .map(|fragment| {
                        format!(
                            "{} ({}) — provider fragment\n{}",
                            fragment.property, fragment.object_type, fragment.fragment
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n"),
                64 * 1024,
            )
        };
        let metadata = Metadata {
            actors: item.author.iter().cloned().collect(),
            state: (!item.state.is_empty()).then(|| item.state.clone()),
            created_at: (!item.created_at.is_empty()).then(|| item.created_at.clone()),
            updated_at: (!item.updated_at.is_empty()).then(|| item.updated_at.clone()),
            fetched_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| QueryError(error.to_string()))?
                .as_secs(),
            complete: false,
            ..Metadata::default()
        };
        let document = Document {
            key: DocumentKey {
                scope: scope.clone(),
                repository: item.repository.clone(),
                revision,
                kind: result_kind,
                path,
                resource: format!("github-search:{}", item.id),
            },
            source: SourceKind::GitHub,
            title,
            body,
            metadata,
            destination,
        };
        document.validate(scope)?;
        let first = item.text_matches.first();
        let snippet = first.map_or_else(
            || bounded(&item.body, 1000),
            |fragment| bounded(&fragment.fragment, 1000),
        );
        let highlights = first
            .filter(|fragment| fragment.fragment.is_ascii())
            .map(|fragment| {
                fragment
                    .server_match_indices
                    .iter()
                    .filter_map(|range| {
                        let start = usize::try_from(range[0]).ok()?;
                        let end = usize::try_from(range[1]).ok()?;
                        (start < end && end <= snippet.len()).then_some(start..end)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let matched_fields = if item.text_matches.is_empty() {
            vec!["GitHub provider match; no fragment metadata returned".into()]
        } else {
            item.text_matches
                .iter()
                .map(|fragment| {
                    format!(
                        "GitHub fragment: {} ({})",
                        fragment.property, fragment.object_type
                    )
                })
                .collect()
        };
        if !page.coverage.repositories.contains(&item.repository) {
            page.coverage.repositories.push(item.repository.clone());
        }
        items.insert(document.key.stable_id(), item);
        page.hits.push(SearchHit {
            document: Arc::new(document),
            line: 0,
            column: 0,
            snippet,
            highlights,
            score: 1.0 / (rank + 1) as f32,
            matched_fields,
        });
    }
    page.coverage.repositories.sort();
    page.coverage.reasons.push("Provider fragments do not supply exact full-file lines; code Open verifies the indexed blob at an immutable commit first. Unicode server offsets are retained without guessing their units.".into());
    Ok((page, items))
}

fn bounded(value: &str, bytes: usize) -> String {
    let mut end = value.len().min(bytes);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}
