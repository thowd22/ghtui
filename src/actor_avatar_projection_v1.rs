//! Borrowed display projection, never account/access/Git-association authority.
//! Preflight a bounded selected batch BEFORE any owned Actor/identity copies.
use crate::{
    app::{App, Overlay, View},
    identity::{ActorIdentity, ActorKind},
    providers::gh::{Actor, HostIdentity},
};

pub const ROWS: usize = 128;
pub const FIELD_BYTES: usize = 4096;
pub const BATCH_BYTES: usize = 64 * 1024;
const CONTEXT_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectionUnavailable;
impl std::fmt::Display for ProjectionUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Actor image metadata unavailable within display bounds; original observations remain unchanged")
    }
}
impl std::error::Error for ProjectionUnavailable {}

pub fn actor_bytes(actor: &Actor) -> Result<usize, ProjectionUnavailable> {
    let mut bytes = 0usize;
    for field in [
        Some(actor.login.as_str()),
        Some(actor.actor_type.as_str()),
        actor.display_name.as_deref(),
        actor.avatar_url.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if field.len() > FIELD_BYTES {
            return Err(ProjectionUnavailable);
        }
        bytes = bytes
            .checked_add(field.len())
            .ok_or(ProjectionUnavailable)?;
    }
    Ok(bytes)
}
/// No original Actor field is cloned until ALL selected references pass. The
/// source iterator is not visited after128; fields/rows are never cropped into
/// new identities. Failure returns no partial cloned batch or permission fact.
pub fn collect<'a>(
    observed: impl IntoIterator<Item = &'a Actor>,
) -> Result<Vec<Actor>, ProjectionUnavailable> {
    let mut references = Vec::with_capacity(ROWS);
    for actor in observed.into_iter().take(ROWS) {
        references.push(actor);
    }
    clone_checked(references)
}
fn clone_checked(references: Vec<&Actor>) -> Result<Vec<Actor>, ProjectionUnavailable> {
    let mut bytes = 0usize;
    for actor in &references {
        let size = actor_bytes(actor)?;
        if size > BATCH_BYTES.saturating_sub(bytes) {
            return Err(ProjectionUnavailable);
        }
        bytes += size;
    }
    Ok(references.into_iter().cloned().collect())
}
/// Full observed equality, after successful bounded projection only. Parent
/// compatibility APIs can retain dedup without collapsing URL/name variants.
pub fn distinct(actors: Vec<Actor>) -> Vec<Actor> {
    let mut unique = Vec::with_capacity(actors.len());
    for actor in actors {
        if !unique.contains(&actor) {
            unique.push(actor)
        }
    }
    unique
}

/// Display refusal only: preserve the raw source Actor, do not shorten its URL
/// or claim a known login/type/host association from only part of an oversized
/// record. Runs BEFORE existing ActorIdentity::github sanitization/copies.
pub fn display_fallback(host: &str, actor: &Actor) -> Option<ActorIdentity> {
    (host.len() > FIELD_BYTES || actor_bytes(actor).is_err()).then(|| ActorIdentity {
        host: None,
        login: None,
        name: String::new(),
        email: None,
        avatar_url: None,
        kind: ActorKind::Unknown,
        git_role: None,
        host_associated: false,
    })
}

fn context_preflight(
    identity: &HostIdentity,
    fields: impl IntoIterator<Item = impl AsRef<str>>,
) -> bool {
    if [&identity.host, &identity.login, &identity.gh_version]
        .iter()
        .any(|field| field.len() > FIELD_BYTES)
    {
        return false;
    }
    let mut bytes = identity
        .host
        .len()
        .saturating_add(identity.login.len())
        .saturating_add(256);
    for field in fields {
        let field = field.as_ref();
        if field.len() > FIELD_BYTES
            || field.len().saturating_add(8) > CONTEXT_BYTES.saturating_sub(bytes)
        {
            return false;
        }
        bytes += field.len() + 8;
    }
    true
}

pub fn github(
    workspace: &crate::github_workspace::GithubWorkspace,
    reactions: Option<&crate::pr_actions::PrReactionPage>,
) -> Result<Vec<Actor>, ProjectionUnavailable> {
    // An owned reaction surface is prioritized; an old hidden reaction page
    // must not extend the current PR/inbox roster or defeat the128-row bound.
    let reactions = reactions
        .into_iter()
        .flat_map(|page| page.reactions.iter().map(|reaction| &reaction.actor));
    if let Some(page) = &workspace.pr {
        collect(
            reactions
                .chain(std::iter::once(&page.details.summary.pr.author))
                .chain(page.rows.iter().filter_map(|row| row.actor.as_ref()))
                .chain(page.details.comments.iter().map(|comment| &comment.author))
                .chain(
                    page.details
                        .commits
                        .iter()
                        .flat_map(|commit| [&commit.author, &commit.committer]),
                ),
        )
    } else {
        collect(
            reactions.chain(
                workspace
                    .inbox
                    .pull_requests
                    .iter()
                    .filter(|pr| {
                        workspace.filter.accepts(pr)
                            && !workspace.exclusions.contains(&pr.repository)
                    })
                    .map(|pr| &pr.author),
            ),
        )
    }
}
pub fn actions(
    workspace: &crate::actions_workspace::ActionsWorkspace,
) -> Result<Vec<Actor>, ProjectionUnavailable> {
    collect(
        workspace
            .rows
            .iter()
            .filter_map(|row| row.actor.as_ref())
            .chain(
                workspace
                    .runs
                    .iter()
                    .flat_map(|run| [&run.run.actor, &run.run.triggering_actor]),
            )
            .chain(
                workspace
                    .run
                    .iter()
                    .flat_map(|run| [&run.attempt.run.actor, &run.attempt.run.triggering_actor]),
            ),
    )
}
pub fn search(
    workspace: &crate::search::workspace::SearchWorkspace,
) -> Result<Vec<Actor>, ProjectionUnavailable> {
    use crate::search::model::SourceKind;
    let Some(page) = &workspace.page else {
        return Ok(vec![]);
    };
    let identity = workspace
        .remote_identity
        .as_ref()
        .ok_or(ProjectionUnavailable)?;
    crate::search::remote::verify_scope(&workspace.scope, identity)
        .map_err(|_| ProjectionUnavailable)?;
    let mut observed = Vec::with_capacity(ROWS);
    for hit in page.hits.iter().take(ROWS) {
        match hit.document.source {
            SourceKind::Collaboration => {
                for actor in &hit.document.metadata.actors {
                    if observed.len() == ROWS {
                        break;
                    }
                    observed.push(actor);
                }
            }
            SourceKind::GitHub => {
                let key = &hit.document.key;
                if [
                    &key.scope.host,
                    &key.scope.login,
                    &key.repository,
                    &key.revision,
                    &key.resource,
                ]
                .iter()
                .any(|field| field.len() > FIELD_BYTES)
                    || key
                        .path
                        .as_ref()
                        .is_some_and(|path| path.0.len() > FIELD_BYTES)
                {
                    return Err(ProjectionUnavailable);
                }
                let item = workspace
                    .remote_items
                    .get(&hit.document.key.stable_id())
                    .ok_or(ProjectionUnavailable)?;
                for actor in [&item.author, &item.repository_owner].into_iter().flatten() {
                    if observed.len() == ROWS {
                        break;
                    }
                    observed.push(actor);
                }
            }
            _ => {}
        }
        if observed.len() == ROWS {
            break;
        }
    }
    clone_checked(observed)
}

/// Active-surface adapters borrow the actual source models. The existing Main
/// helper still applies positive caller/cache metadata and live retirement.
pub fn active(app: &App) -> Option<(HostIdentity, String, Vec<Actor>)> {
    match app.view {
        View::Repository => None,
        View::Inbox => {
            if let Some(view) = app
                .scoped_threads_v1
                .as_ref()
                .filter(|view| app.overlay == Some(Overlay::PrThreadsV1) && view.intent.owns(app))
            {
                let prepared = view.prepared.as_ref()?;
                let actors = collect(
                    prepared
                        .observation
                        .threads
                        .iter()
                        .flat_map(|thread| {
                            thread.comments.iter().filter_map(|comment| {
                                comment.author.as_ref().map(|actor| &actor.actor)
                            })
                        })
                        .chain(prepared.observation.threads.iter().filter_map(|thread| {
                            thread.resolved_by.as_ref().map(|actor| &actor.actor)
                        })),
                )
                .ok()?;
                return Some((
                    prepared.identity.clone(),
                    "Original scoped PR thread observations".into(),
                    actors,
                ));
            }
            let workspace = app.github.as_ref()?;
            if !context_preflight(&workspace.identity, &workspace.repositories) {
                return None;
            }
            let reactions = (app.overlay == Some(Overlay::PrReactions))
                .then_some(app.pr_reactions.as_ref())
                .flatten();
            Some((
                workspace.identity.clone(),
                workspace.context(),
                github(workspace, reactions).ok()?,
            ))
        }
        View::Actions => {
            let workspace = app.actions.as_ref()?;
            if !context_preflight(&workspace.identity, &workspace.repositories) {
                return None;
            }
            Some((
                workspace.identity.clone(),
                workspace.context(),
                actions(workspace).ok()?,
            ))
        }
        View::Search => {
            let workspace = app.search.as_ref()?;
            let identity = workspace.remote_identity.as_ref()?;
            let fields = workspace
                .remote_repositories
                .iter()
                .map(String::as_str)
                .chain(
                    workspace
                        .selections
                        .iter()
                        .map(|selection| selection.repository.as_str()),
                )
                .chain(workspace.selections.iter().filter_map(
                    |selection| match &selection.source {
                        crate::search::corpus::LocalSource::Revision(revision) => {
                            Some(revision.as_str())
                        }
                        _ => None,
                    },
                ))
                .chain(
                    workspace
                        .metadata_sources
                        .discussions
                        .iter()
                        .map(|target| target.repository.as_str()),
                )
                .chain(
                    workspace
                        .metadata_sources
                        .commits
                        .iter()
                        .flat_map(|target| [target.repository.as_str(), target.revision.as_str()]),
                );
            if !context_preflight(identity, fields) {
                return None;
            }
            Some((
                identity.clone(),
                workspace.context(),
                search(workspace).ok()?,
            ))
        }
        View::Dashboard => {
            use crate::metrics_workspace::MetricsView;
            let workspace = app
                .metrics
                .as_ref()
                .filter(|workspace| workspace.pending.is_none())?;
            if workspace.snapshot.scope.host().len() > FIELD_BYTES
                || workspace.snapshot.scope.login().len() > FIELD_BYTES
            {
                return None;
            }
            let identity = HostIdentity {
                host: workspace.snapshot.scope.host().into(),
                login: workspace.snapshot.scope.login().into(),
                account_id: Some(workspace.snapshot.scope.account_id()),
                gh_version: String::new(),
            };
            if !context_preflight(&identity, [&workspace.scope_label, &workspace.as_of_label]) {
                return None;
            }
            let actors = match workspace.view {
                MetricsView::Contributors(card) => collect(
                    workspace
                        .snapshot
                        .cards
                        .get(card)?
                        .contributors
                        .iter()
                        .skip(workspace.top)
                        .take(app.metrics_visible_rows.max(1))
                        .flat_map(|contributor| {
                            contributor.actors.iter().map(|actor| &actor.actor)
                        }),
                ),
                MetricsView::Details { card, contributor } => collect(
                    workspace
                        .snapshot
                        .cards
                        .get(card)?
                        .contributors
                        .get(contributor)?
                        .actors
                        .iter()
                        .map(|actor| &actor.actor),
                ),
                _ => Ok(vec![]),
            }
            .ok()?;
            Some((
                identity,
                format!(
                    "Dashboard / {} / {}",
                    workspace.scope_label, workspace.as_of_label
                ),
                actors,
            ))
        }
    }
}

#[cfg(test)]
#[path = "actor_avatar_projection_v1_tests.rs"]
mod tests;
