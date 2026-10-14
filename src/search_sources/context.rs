//! Worker-prepared exact discussion/commit context. No browser escape hatch or invented lines.
use crate::{
    identity::{ActorIdentity, GitRole},
    providers::runner::CancellationToken,
    search::{
        commits,
        model::{Destination, DocumentKey},
        workspace::SearchWorkspace,
    },
    storage::{Cache, Scope},
};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone)]
pub struct SourceContext {
    pub key: DocumentKey,
    pub scope: Scope,
    pub rows: Vec<String>,
    pub actors: Vec<ActorIdentity>,
}

pub fn prepare(
    search: &SearchWorkspace,
    index: usize,
    cache: &Cache,
    cancel: &CancellationToken,
) -> Result<SourceContext, String> {
    let document = search
        .result_document(index)
        .map_err(|error| error.to_string())?;
    document
        .validate(&search.scope)
        .map_err(|error| error.to_string())?;
    search
        .validate_metadata_result(index, cache, cancel)
        .map_err(|error| error.to_string())?;
    if let Destination::LocalCommit {
        directory,
        commit_id,
    } = &document.destination
    {
        let selection = commits::CommitSelection {
            repository: document.key.repository.clone(),
            directory: directory.clone(),
            revision: commit_id.clone(),
            max_commits: 1,
        };
        let mut current = commits::collect(
            search.scope.clone(),
            &[selection],
            &search.commit_limits,
            cancel,
        )
        .map_err(|error| error.to_string())?;
        let found = current
            .corpus
            .documents
            .iter_mut()
            .find(|current| current.key == document.key)
            .ok_or("The exact original commit is unavailable; no different version was opened")?;
        if let Some(selection) = search
            .selections
            .iter()
            .find(|selection| selection.repository == found.key.repository)
        {
            found.metadata.repository_sets = selection.sets.clone();
        }
        if found
            .projection_checksum()
            .map_err(|error| error.to_string())?
            != document
                .projection_checksum()
                .map_err(|error| error.to_string())?
        {
            return Err("Original commit context changed; run the source search again".into());
        }
    } else if document.metadata.discussion.is_none() {
        return Err("This source is not a canonical discussion or original commit".into());
    }
    let mut rows = Vec::new();
    let mut add = |text: String| rows.extend(wrap(&text));
    add(format!(
        "{} / {} / {}",
        document.key.scope.host,
        document.key.repository,
        document.source.label()
    ));
    if let Some(context) = &document.metadata.discussion {
        add(format!(
            "{} #{} • {}",
            context.kind.label(),
            context.parent_number,
            document.title
        ));
        add(format!("Parent node: {}", context.parent_node));
        add(format!("Resource node: {}", context.resource_node));
        if let Some(thread) = &context.thread_id {
            add(format!("Thread node: {thread}"));
        }
        if let Some(commit) = &context.commit_sha {
            add(format!("Anchor saved version: {commit}"));
        }
        if let Some(path) = &context.path {
            add(format!(
                "Diff file: {} • side {} • lines {}..{}",
                path.display(),
                context.side.as_deref().unwrap_or("unknown"),
                context
                    .start_line
                    .or(context.line)
                    .map_or_else(|| "unknown".into(), |line| line.to_string()),
                context
                    .line
                    .map_or_else(|| "unknown".into(), |line| line.to_string())
            ));
        }
        if let Some(resolved) = context.resolved {
            add(format!(
                "Resolved: {resolved} • outdated: {}",
                context.outdated
            ));
        }
        add(format!(
            "Source updated: {} • fetched: {}",
            document
                .metadata
                .updated_at
                .as_deref()
                .unwrap_or("unavailable"),
            document.metadata.fetched_at
        ));
        add(format!(
            "Cached {} source • body line numbers below are NOT diff anchors",
            if document.metadata.complete {
                "complete selected"
            } else {
                "partial selected"
            }
        ));
        if let Some(url) = &context.url {
            add(format!("Source URL: {url}"));
        }
    } else {
        add(format!("Original saved version: {}", document.key.revision));
        add("Read-only commit message • no checkout, fetch or Git changes".into());
    }
    add("".into());
    for (number, line) in document.body.lines().enumerate() {
        add(format!("{:>5} │ {line}", number + 1));
    }
    let mut actors = Vec::new();
    if let Some(author) = &document.metadata.git_author {
        actors.push(ActorIdentity::local(author, GitRole::Author));
    }
    if let Some(committer) = &document.metadata.git_committer {
        actors.push(ActorIdentity::local(committer, GitRole::Committer));
    }
    if document.metadata.discussion.is_some() {
        let actor = document
            .metadata
            .actors
            .first()
            .cloned()
            .unwrap_or_else(|| crate::providers::gh::Actor {
                actor_type: "Unknown".into(),
                ..Default::default()
            });
        actors.push(ActorIdentity::github(&search.scope.host, &actor));
    }
    Ok(SourceContext {
        key: document.key.clone(),
        scope: search.scope.clone(),
        rows,
        actors,
    })
}

// Fixed68-cell worker preparation allows120x40 and80x24 to share exact prepared rows.
fn wrap(text: &str) -> Vec<String> {
    let safe = crate::app::safe_text(text);
    let mut result = Vec::new();
    for line in safe.split('\n') {
        let mut row = String::new();
        let mut width = 0;
        for grapheme in line.graphemes(true) {
            let next = crate::editor::grapheme_width(grapheme);
            if width + next > 68 && !row.is_empty() {
                result.push(std::mem::take(&mut row));
                width = 0;
            }
            row.push_str(grapheme);
            width += next;
        }
        result.push(row);
    }
    result
}
