//! Closed actual Inbox origin for State source recovery. Capturing this is
//! read intent and a local restriction, never PR membership/access or a Base.
//! No parent can be reconstructed from its queue IDs or journal rows.
use crate::{
    active_remote_scope_v1::{ActiveRemoteScopeV1, RemoteScopeConsumerV1},
    app::{App, View},
    github_workspace::{GithubWorkspace, InboxFilter, LegacyStartupQueueV80, PrKey},
    providers::{gh::HostIdentity, runner::CancellationToken},
    storage::{CatalogueIntentSource, Scope, StateSourceEpochV76},
};
use std::{
    collections::BTreeSet,
    mem::size_of,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

const RETAINED: usize = 8 * 1024 * 1024;
enum FamilyV76 {
    LegacyStartup(Arc<LegacyStartupQueueV80>),
    Personal(Arc<ActiveRemoteScopeV1>),
    Catalogue {
        active: Arc<ActiveRemoteScopeV1>,
        source: Arc<CatalogueIntentSource>,
    },
}
pub(crate) enum StateSourceOriginV80<'a> {
    LegacyStartup,
    Personal(&'a Arc<ActiveRemoteScopeV1>),
    Catalogue {
        active: &'a Arc<ActiveRemoteScopeV1>,
        source: &'a Arc<CatalogueIntentSource>,
    },
}
pub(crate) struct StateInboxOriginalV76 {
    family: FamilyV76,
    identity: HostIdentity,
    scope: Scope,
    database: PathBuf,
    source: String,
    repositories: Vec<String>,
    exclusions: BTreeSet<String>,
    rows: (usize, usize, usize),
    generation: u64,
    selected: Option<PrKey>,
    filter: InboxFilter,
    position: (usize, usize, usize, bool),
    held_graph: usize,
    cancel: CancellationToken,
    deadline: Instant,
}
/// NonClone/nonSerde; only capture from the actual live App below can issue it.
pub(crate) struct StateSourceSyncV76 {
    original: Arc<StateInboxOriginalV76>,
    epoch: StateSourceEpochV76,
}
fn unavailable() -> String {
    "Actual bounded State Inbox source unavailable; no default/narrower query or permission inferred".into()
}
fn node_bound<T>(entries: usize) -> Option<usize> {
    entries.checked_mul(2)?.checked_add(1)?.checked_mul(
        32usize.checked_add(16usize.checked_mul(size_of::<T>() + size_of::<usize>())?)?,
    )
}
fn identity_bytes(identity: &HostIdentity) -> Option<usize> {
    identity
        .host
        .capacity()
        .checked_add(identity.login.capacity())?
        .checked_add(identity.gh_version.capacity())
}
fn string_set_bytes(set: &BTreeSet<String>) -> Option<usize> {
    set.iter()
        .try_fold(node_bound::<String>(set.len())?, |n, value| {
            n.checked_add(value.capacity())
        })
}
/// ACTUAL retained queue/actor/coverage capacities, not serialized lengths or
/// a small Form standing in for the separately retained source graph.
fn workspace_bytes(g: &GithubWorkspace) -> Option<usize> {
    let mut n = size_of::<GithubWorkspace>()
        .checked_add(g.legacy_queue_retained_bytes_v80()?)?
        .checked_add(g.state_route_retained_bytes_v80())?
        .checked_add(identity_bytes(&g.identity)?)?
        .checked_add(identity_bytes(&g.inbox.identity)?)?
        .checked_add(g.repositories.capacity().checked_mul(size_of::<String>())?)?
        .checked_add(string_set_bytes(&g.exclusions)?)?
        .checked_add(
            g.inbox
                .pull_requests
                .capacity()
                .checked_mul(size_of::<crate::providers::gh::PullRequest>())?,
        )?
        .checked_add(
            g.inbox
                .coverage
                .reasons
                .capacity()
                .checked_mul(size_of::<String>())?,
        )?;
    for value in g.repositories.iter().chain(g.inbox.coverage.reasons.iter()) {
        n = n.checked_add(value.capacity())?;
    }
    if let Some(key) = &g.selected {
        n = n
            .checked_add(key.host.capacity())?
            .checked_add(key.repository.capacity())?;
    }
    if let crate::github_workspace::Freshness::Offline { error, .. } = &g.freshness {
        n = n.checked_add(error.capacity())?;
    }
    for pr in &g.inbox.pull_requests {
        for value in [
            &pr.host,
            &pr.id,
            &pr.repository,
            &pr.title,
            &pr.body,
            &pr.url,
            &pr.state,
            &pr.created_at,
            &pr.updated_at,
            &pr.author.login,
            &pr.author.actor_type,
        ] {
            n = n.checked_add(value.capacity())?;
        }
        n = n
            .checked_add(pr.author.display_name.as_ref().map_or(0, String::capacity))?
            .checked_add(pr.author.avatar_url.as_ref().map_or(0, String::capacity))?
            .checked_add(node_bound::<crate::providers::gh::Queue>(pr.queues.len())?)?;
    }
    Some(n)
}
fn opposite_intent(app: &App, own_refresh: bool) -> bool {
    app.github_action.is_some()
        || (!own_refresh && app.refresh_requested)
        || app.pr_action.is_some()
        || app.cancel_requested
        || app.scoped_pr_tab_cancel_requested
        || app.personal_pr_entry_work_v13.is_some()
        || app.personal_pr_entry_cancel_v13
        || app.scoped_pr_collaboration_work_v17.is_some()
        || app.scoped_pr_collaboration_cancel_v17
        || app.notifications_work_v63.is_some()
        || app.notifications_owner_v63.is_some()
        || app.notifications_cancel_v63
        || app.scoped_inline_work_v1.is_some()
        || app.scoped_inline_cancel_v1
        || app.scoped_threads_work_v1.is_some()
        || app.scoped_threads_cancel_v1
        || app.scoped_reviews_work_v2.is_some()
        || app.scoped_reviews_cancel_v2
        || app.pr_native_diff_work_v1.is_some()
        || app.pr_native_diff_cancel_v1
        || app.scoped_actions_work_v1.is_some()
        || app.scoped_actions_cancel_v1
}
impl StateInboxOriginalV76 {
    fn capture(app: &App, database: &Path) -> Result<Arc<Self>, String> {
        let cancel = CancellationToken::default();
        let deadline = Instant::now() + Duration::from_secs(30);
        if app.view != View::Inbox
            || app.form.is_some()
            || app.overlay.is_some()
            || app.loading[View::Inbox.index()]
            || opposite_intent(app, true)
            || app.scoped_pr_collaboration_owner_v17.is_some()
            || app.personal_pr_source_v13.is_some()
            || app.notification_pr_source_v63.is_some()
            || database.as_os_str().len() > 4096
        {
            return Err(unavailable());
        }
        let g = app.github.as_ref().ok_or_else(unavailable)?;
        if g.pr.is_some()
            || g.scoped_pr_navigation.is_some()
            || g.scoped_pr_resource.is_some()
            || g.personal_pr_page_ticket_v13.is_some()
            || g.notification_pr_page_ticket_v63.is_some()
            // Entire actual source refuses, never silently select two. This
            // is the explicitly bounded V76 intermediate, not full org scope.
            || g.repositories.len() > 2
            || g.identity
                .account_id
                .is_none_or(|n| n == 0 || n > i64::MAX as u64)
            || g.identity.host.is_empty()
            || g.identity.host.len() > 253
            || g.identity.login.is_empty()
            || g.identity.login.len() > 100
            || g.identity.gh_version.len() > 256
            || !g.identity.same_account(&g.inbox.identity)
        {
            return Err(unavailable());
        }
        // Walk complete ACTUAL borrowed allocations before ANY owned queue
        // serializer, identity, selected-key, repositories or source clone.
        let mut graph = workspace_bytes(g).ok_or_else(unavailable)?;
        let route = match app.remote_scope.as_ref() {
            Some(active) => {
                graph = graph
                    .checked_add(active.workflow_owned_bytes_v9().ok_or_else(unavailable)?)
                    .ok_or_else(unavailable)?;
                match active.consumer(&g.identity) {
                    RemoteScopeConsumerV1::DefaultPersonal
                        if app.remote_scope_source.is_none() && g.repositories.is_empty() =>
                    {
                        StateSourceOriginV80::Personal(active)
                    }
                    RemoteScopeConsumerV1::Explicit(refs) => {
                        if refs.references().is_empty() || refs.references().len() > 2 {
                            return Err(unavailable());
                        }
                        let source = app.remote_scope_source.as_ref().ok_or_else(unavailable)?;
                        if g.scope().ok().as_ref() != Some(source.scope())
                            || g.repositories.len() != refs.references().len()
                            || refs.references().iter().any(|r| {
                                r.observed().is_none_or(|observed| {
                                    !g.repositories
                                        .iter()
                                        .any(|name| name == &observed.last_name)
                                })
                            })
                        {
                            return Err(unavailable());
                        }
                        graph = graph
                            .checked_add(source.workflow_owned_bytes_v9().ok_or_else(unavailable)?)
                            .ok_or_else(unavailable)?;
                        StateSourceOriginV80::Catalogue { active, source }
                    }
                    _ => return Err(unavailable()),
                }
            }
            None if app.remote_scope_source.is_none() && app.catalogue.is_none() => {
                // Positive producer lineage, never `None => default`. Empty,
                // cached-only and scoped Workspaces have no such stamp.
                g.legacy_queue_v80().ok_or_else(unavailable)?;
                // Actual retained stamp allocation was already charged by
                // workspace_bytes, including invalid-but-still-held stamps.
                StateSourceOriginV80::LegacyStartup
            }
            None => return Err(unavailable()),
        };
        let fields = g
            .repositories
            .len()
            .checked_add(g.exclusions.len())
            .filter(|n| *n <= 512)
            .ok_or_else(unavailable)?;
        let text = g
            .repositories
            .iter()
            .chain(g.exclusions.iter())
            .try_fold(0usize, |n, s| n.checked_add(s.capacity()))
            .ok_or_else(unavailable)?;
        // Bound same source() sorting/JSON escape overlap BEFORE invoking it.
        let copies = text
            .checked_mul(24)
            .and_then(|n| n.checked_add(fields.checked_mul(1024)?))
            .and_then(|n| n.checked_add(4096))
            .filter(|n| *n <= 64 * 1024)
            .ok_or_else(unavailable)?;
        // The second actual-graph charge conservatively covers every copied
        // identity/key/vector subset while the original App graph still lives;
        // it does NOT deduplicate retained Source/Active Arc allocations.
        let held_graph = graph
            .checked_mul(2)
            .and_then(|n| n.checked_add(copies))
            .and_then(|n| {
                n.checked_add(
                    size_of::<Self>()
                        + size_of::<StateSourceSyncV76>()
                        + 4 * size_of::<usize>()
                        + 64 * 1024,
                )
            })
            .filter(|n| *n <= RETAINED)
            .ok_or_else(unavailable)?;
        if cancel.is_cancelled() || Instant::now() >= deadline {
            return Err(unavailable());
        }
        let source = g.source();
        if source.is_empty() || source.len() > 2048 || source.capacity() > 4096 {
            return Err(unavailable());
        }
        let captured = Arc::new(Self {
            family: match route {
                StateSourceOriginV80::LegacyStartup => {
                    let startup = g.legacy_queue_v80().ok_or_else(unavailable)?;
                    startup
                        .validate(g, database, &cancel, deadline)
                        .map_err(|_| unavailable())?;
                    FamilyV76::LegacyStartup(Arc::clone(startup))
                }
                StateSourceOriginV80::Personal(active) => FamilyV76::Personal(Arc::clone(active)),
                StateSourceOriginV80::Catalogue { active, source } => FamilyV76::Catalogue {
                    active: Arc::clone(active),
                    source: Arc::clone(source),
                },
            },
            identity: g.identity.clone(),
            scope: g.scope().map_err(|_| unavailable())?,
            database: database.into(),
            source,
            repositories: g.repositories.clone(),
            exclusions: g.exclusions.clone(),
            rows: (
                g.inbox.pull_requests.as_ptr() as usize,
                g.inbox.pull_requests.len(),
                g.inbox.pull_requests.capacity(),
            ),
            generation: g.generation,
            selected: g.selected.clone(),
            filter: g.filter,
            position: (
                app.selected,
                app.scroll,
                app.focus,
                app.github_content_focused,
            ),
            held_graph,
            cancel,
            deadline,
        });
        if !captured.matches(app, true) {
            return Err(unavailable());
        }
        Ok(captured)
    }
    pub(crate) fn scope(&self) -> &Scope {
        &self.scope
    }
    pub(crate) fn source(&self) -> &str {
        &self.source
    }
    pub(crate) fn database(&self) -> &Path {
        &self.database
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub(crate) fn repositories(&self) -> &[String] {
        &self.repositories
    }
    pub(crate) fn exclusions(&self) -> &BTreeSet<String> {
        &self.exclusions
    }
    pub(crate) fn selection_v80(&self) -> (InboxFilter, &Option<PrKey>) {
        (self.filter, &self.selected)
    }
    pub(crate) fn origin(&self) -> StateSourceOriginV80<'_> {
        match &self.family {
            FamilyV76::LegacyStartup(_) => StateSourceOriginV80::LegacyStartup,
            FamilyV76::Personal(active) => StateSourceOriginV80::Personal(active),
            FamilyV76::Catalogue { active, source } => {
                StateSourceOriginV80::Catalogue { active, source }
            }
        }
    }
    pub(crate) fn held_graph(&self) -> usize {
        self.held_graph
    }
    pub(crate) fn cancellation(&self) -> &CancellationToken {
        &self.cancel
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }
    fn matches(&self, app: &App, own_refresh: bool) -> bool {
        app.view == View::Inbox
            && app.form.is_none()
            && app.overlay.is_none()
            && app.personal_pr_source_v13.is_none()
            && app.notification_pr_source_v63.is_none()
            && !opposite_intent(app, own_refresh)
            && match (&self.family, &app.remote_scope, &app.remote_scope_source) {
                (FamilyV76::LegacyStartup(a), None, None) => {
                    app.catalogue.is_none()
                        && app
                            .github
                            .as_ref()
                            .and_then(GithubWorkspace::legacy_queue_v80)
                            .is_some_and(|b| Arc::ptr_eq(a, b))
                }
                (FamilyV76::Personal(a), Some(b), None) => {
                    Arc::ptr_eq(a, b)
                        && matches!(
                            b.consumer(&self.identity),
                            RemoteScopeConsumerV1::DefaultPersonal
                        )
                }
                (
                    FamilyV76::Catalogue {
                        active: a,
                        source: x,
                    },
                    Some(b),
                    Some(y),
                ) => {
                    Arc::ptr_eq(a, b)
                        && Arc::ptr_eq(x, y)
                        && matches!(
                            b.consumer(&self.identity),
                            RemoteScopeConsumerV1::Explicit(_)
                        )
                }
                _ => false,
            }
            && (
                app.selected,
                app.scroll,
                app.focus,
                app.github_content_focused,
            ) == self.position
            && app.github.as_ref().is_some_and(|g| {
                g.pr.is_none()
                    && g.scoped_pr_navigation.is_none()
                    && g.scoped_pr_resource.is_none()
                    && g.personal_pr_page_ticket_v13.is_none()
                    && g.notification_pr_page_ticket_v63.is_none()
                    && self.identity.same_account(&g.identity)
                    && g.generation == self.generation
                    && g.filter == self.filter
                    && g.selected == self.selected
                    && g.repositories == self.repositories
                    && g.exclusions == self.exclusions
                    && (
                        g.inbox.pull_requests.as_ptr() as usize,
                        g.inbox.pull_requests.len(),
                        g.inbox.pull_requests.capacity(),
                    ) == self.rows
            })
    }
}
impl StateSourceSyncV76 {
    pub(crate) fn matches_capture_v80(&self, app: &App) -> bool {
        !self.original.cancel.is_cancelled()
            && Instant::now() < self.original.deadline
            && self.original.matches(app, true)
            && app.scoped_pr_collaboration_owner_v17.is_none()
            && !app.loading[View::Inbox.index()]
    }
    pub(crate) fn capture(app: &App, database: &Path) -> Result<Arc<Self>, String> {
        let original = StateInboxOriginalV76::capture(app, database)?;
        let epoch = StateSourceEpochV76::capture(&original).map_err(|_| unavailable())?;
        if !original.matches(app, true) {
            return Err(unavailable());
        }
        Ok(Arc::new(Self { original, epoch }))
    }
    pub(crate) fn original(&self) -> &Arc<StateInboxOriginalV76> {
        &self.original
    }
    pub(crate) fn owns(&self, app: &App) -> bool {
        !self.original.cancel.is_cancelled()
            && Instant::now() < self.original.deadline
            && self.original.matches(app, false)
    }
    pub(crate) fn validate_original(&self) -> Result<(), String> {
        if self.original.cancel.is_cancelled() || Instant::now() >= self.original.deadline {
            return Err(unavailable());
        }
        self.epoch
            .validate_original(&self.original)
            .map_err(|_| unavailable())
    }
    /// SAME fixed raw predecessor validator; no caller-selected SQL/callback,
    /// generation or Source constructor. Only the closed storage kernel uses it.
    pub(crate) fn validate_prepared_transaction_v80(
        &self,
        transaction: &rusqlite::Connection,
    ) -> Result<(), String> {
        if self.original.cancel.is_cancelled() || Instant::now() >= self.original.deadline {
            return Err(unavailable());
        }
        self.epoch
            .validate_transaction(&self.original, transaction)
            .map_err(|_| unavailable())
    }
}
