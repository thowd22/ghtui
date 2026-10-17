//! One genuine State Inbox read. This is NOT a PR parent/access receipt and
//! cannot be used by a writer. Its opaque close is useful only to the exact
//! preencoded State source transaction; observed rows alone remain data.
use super::{state_source_heap_v80::*, *};
use crate::{
    active_remote_scope_v1::RemoteScopeConsumerV1,
    github_workspace::GithubWorkspace,
    providers::runner::ScopedReadBudgetV1,
    state_inbox_sync_v76::{StateSourceOriginV80, StateSourceSyncV76},
};
use std::{collections::BTreeMap, mem::size_of};

const WHOLE: usize = 8 * 1024 * 1024;
const QUEUE_CAP: usize = 120 * 1024;
const REPOSITORY_CAP: usize = 128 * 1024;
const PRINCIPAL_CAP: usize = 68 * 1024;
fn unavailable() -> GithubError {
    error(
        GithubErrorKind::IdentityChanged,
        "Original complete State source changed; old membership remains unavailable",
    )
}
fn schema() -> GithubError {
    error(
        GithubErrorKind::Json,
        "State queue/repository schema or complete coverage unavailable; no membership published",
    )
}
fn heap_error() -> GithubError {
    error(GithubErrorKind::Transport,
        "State source typed graph exceeds its preallocated phase bound; no next child or publication")
}

struct RepositoryV80 {
    id: u64,
    node: Option<String>,
    name: String,
}
/// Actual opening producer + same runner/clock/pool. Not Clone/serde. The pool
/// aborts on every early return even when an individual child Arc survives.
struct SessionV80 {
    provider: GithubProvider,
    budget: CatalogueBudget,
    original: Arc<StateSourceSyncV76>,
    heap: Arc<StateSourceHeapV80>,
    identity: HostIdentity,
    repositories: Vec<RepositoryV80>,
    pool: Option<Arc<ScopedReadBudgetV1>>,
    charges: Vec<StateSourceChargeV80>,
}
impl Drop for SessionV80 {
    fn drop(&mut self) {
        if let Some(pool) = &self.pool {
            pool.abort();
        }
    }
}
pub(crate) struct StateSourceReadV80 {
    // Real typed models drop before their retained graph debits.
    workspace: GithubWorkspace,
    session: SessionV80,
    binding: Arc<ReadBindingV80>,
}
struct ReadBindingV80;
/// Only a borrow of a genuine pre-final-UID read can issue this restriction.
/// No caller-supplied deadline/generation/raw ID or arbitrary callback.
pub(crate) struct StateSourcePreparedBindingV80 {
    original: Arc<StateSourceSyncV76>,
    binding: std::sync::Weak<ReadBindingV80>,
    deadline: Instant,
}
pub(crate) struct ClosedStateSourceV80 {
    workspace: GithubWorkspace,
    session: SessionV80,
    binding: Arc<ReadBindingV80>,
}
impl StateSourcePreparedBindingV80 {
    pub(crate) fn validate(&self, closed: &ClosedStateSourceV80) -> GithubResult<()> {
        if !Arc::ptr_eq(&self.original, &closed.session.original)
            || !self
                .binding
                .upgrade()
                .is_some_and(|b| Arc::ptr_eq(&b, &closed.binding))
            || self.deadline != closed.session.heap.deadline()
        {
            return Err(unavailable());
        }
        closed.session.heap.check()?;
        let accounting = closed
            .session
            .pool
            .as_ref()
            .ok_or_else(unavailable)?
            .accounting()
            .map_err(GithubError::from)?;
        if !accounting.sealed() {
            return Err(unavailable());
        }
        Ok(())
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }
}
impl StateSourceReadV80 {
    pub(crate) fn read(
        original: Arc<StateSourceSyncV76>,
        provider: &GithubProvider,
    ) -> GithubResult<Self> {
        // This happens BEFORE provider/source/env/model/index/JSON clones.
        let heap = StateSourceHeapV80::before_core(Arc::clone(&original), provider)?;
        let fixed = size_of::<SessionV80>()
            .checked_add(size_of::<Self>())
            .and_then(|n| n.checked_add(72 * size_of::<StateSourceChargeV80>()))
            .and_then(|n| n.checked_add(4 * size_of::<RepositoryV80>()))
            .and_then(|n| n.checked_add(8 * 1024))
            .ok_or_else(heap_error)?;
        let charge = heap.model(fixed)?;
        let limits = RepositoryCatalogueLimits {
            max_pages: 10,
            page_size: 100,
            max_rows: 6000,
            max_document_bytes: REPOSITORY_CAP,
            max_metadata_bytes: WHOLE,
            max_response_bytes: WHOLE,
            max_duration: heap.deadline().saturating_duration_since(Instant::now()),
        };
        limits.validate()?;
        let mut budget = CatalogueBudget::new(&limits)?;
        budget.original_cancellation = Some(original.original().cancellation().clone());
        let mut session = SessionV80 {
            provider: provider.clone(),
            budget,
            original,
            heap,
            identity: HostIdentity {
                host: String::new(),
                login: String::new(),
                account_id: None,
                gh_version: String::new(),
            },
            repositories: Vec::with_capacity(2),
            pool: None,
            charges: Vec::with_capacity(72),
        };
        session.charges.push(charge);
        session.identity = session.principal()?;
        let captured = session.original.original().identity();
        // Query spelling remains the genuinely captured original intent. A
        // changed login is an honest refusal, not an alias/default fallback.
        if session.identity.host != captured.host
            || session.identity.account_id != captured.account_id
            || !session.identity.login.eq_ignore_ascii_case(&captured.login)
        {
            return Err(unavailable());
        }
        let catalogue = matches!(
            session.original.original().origin(),
            StateSourceOriginV80::Catalogue { .. }
        );
        if catalogue {
            session.open_repositories()?;
            // Preserve the existing scoped Inbox transport's inner principal.
            let inner = session.principal()?;
            session.same_identity(&inner)?;
        }
        session.bind_pool(catalogue)?;
        let mut inbox = session.queues()?;
        inbox.pull_requests.retain(|pr| {
            !session
                .original
                .original()
                .exclusions()
                .contains(&pr.repository)
        });
        inbox.coverage.loaded = inbox.pull_requests.len();
        session.heap.check()?;
        // Preflight copies of the actual source configuration before the
        // ordinary Workspace data factory; it creates no ticket/source/access.
        let fields = session
            .original
            .original()
            .repositories()
            .iter()
            .chain(session.original.original().exclusions().iter())
            .try_fold(0usize, |n, s| n.checked_add(s.capacity()))
            .and_then(|n| n.checked_mul(8))
            .and_then(|n| n.checked_add(16 * 1024))
            .ok_or_else(heap_error)?;
        session.charges.push(session.heap.model(fields)?);
        let mut workspace = GithubWorkspace::empty(
            session.identity.clone(),
            session.original.original().repositories().to_vec(),
            session.original.original().exclusions().clone(),
        );
        workspace.inbox = inbox;
        workspace.needs_refresh = false;
        // The actual original filter/selection is applied only by the closed
        // installed result. No caller may expose this unclosed Workspace.
        Ok(Self {
            workspace,
            session,
            binding: Arc::new(ReadBindingV80),
        })
    }
    pub(crate) fn original(&self) -> &Arc<StateSourceSyncV76> {
        &self.session.original
    }
    pub(crate) fn workspace(&self) -> &GithubWorkspace {
        &self.workspace
    }
    pub(crate) fn prepared_binding(&self) -> GithubResult<StateSourcePreparedBindingV80> {
        self.session.heap.check()?;
        Ok(StateSourcePreparedBindingV80 {
            original: Arc::clone(&self.session.original),
            binding: Arc::downgrade(&self.binding),
            deadline: self.session.heap.deadline(),
        })
    }
    pub(crate) fn precharge_storage(&self, bytes: usize) -> GithubResult<StateSourceChargeV80> {
        self.session.heap.model(bytes)
    }
    /// Called only AFTER all encoded rows, keys, new local generation, successor
    /// descriptors and UI storage are prepared. SAME final account bracket.
    pub(crate) fn close(mut self) -> GithubResult<ClosedStateSourceV80> {
        self.session.heap.check()?;
        let pool = self.session.pool.as_ref().ok_or_else(unavailable)?;
        pool.seal(self.session.original.original().cancellation())
            .map_err(GithubError::from)?;
        for index in 0..self.session.repositories.len() {
            let name = &self.session.repositories[index].name;
            let raw = self.session.json(
                StateSourceRoleV80::Repository,
                vec![
                    "api".into(),
                    format!("repos/{name}"),
                    "--hostname".into(),
                    self.session.provider.host.clone(),
                    "--method".into(),
                    "GET".into(),
                ],
            )?;
            validate_repository(raw.value(), &self.session.repositories[index])?;
        }
        let current = self.session.principal()?;
        self.session.same_identity(&current)?;
        self.session.heap.check()?;
        Ok(ClosedStateSourceV80 {
            workspace: self.workspace,
            session: self.session,
            binding: self.binding,
        })
    }
}
impl ClosedStateSourceV80 {
    pub(crate) fn original(&self) -> &Arc<StateSourceSyncV76> {
        &self.session.original
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.session.heap.deadline()
    }
    pub(crate) fn install_prepared_workspace(
        &mut self,
        prepared: crate::github_workspace::PreparedStateWorkspaceV80,
    ) -> GithubResult<()> {
        prepared
            .install(&self.session.original, &mut self.workspace)
            .map_err(|_| unavailable())
    }
    pub(crate) fn into_workspace(self) -> GithubWorkspace {
        self.workspace
    }
}
impl SessionV80 {
    fn same_identity(&self, actual: &HostIdentity) -> GithubResult<()> {
        if actual.host != self.identity.host
            || actual.account_id != self.identity.account_id
            || !actual.login.eq_ignore_ascii_case(&self.identity.login)
        {
            return Err(unavailable());
        }
        Ok(())
    }
    fn json(
        &mut self,
        role: StateSourceRoleV80,
        args: Vec<String>,
    ) -> GithubResult<StateSourceValueV80> {
        self.heap.check()?;
        let capture = self.heap.capture(&self.provider, role)?;
        let raw = self.budget.state_source_command_v80(
            &self.provider,
            args,
            self.original.original().cancellation(),
            &capture,
        )?;
        capture.decode(raw)
    }
    fn principal(&mut self) -> GithubResult<HostIdentity> {
        self.heap.check()?;
        let capture = self
            .heap
            .capture(&self.provider, StateSourceRoleV80::Version)?;
        let raw = self.budget.state_source_command_v80(
            &self.provider,
            vec!["--version".into()],
            self.original.original().cancellation(),
            &capture,
        )?;
        let raw = capture.own_raw(raw)?;
        let version = GhCliVersion::parse_stdout(&raw).map_err(|_| {
            error(
                GithubErrorKind::Unsupported,
                "gh version syntax is unverified; no authenticated State query made",
            )
        })?;
        if !matches!(
            ActualGhTransport::FixedPageApiJson
                .requirement()
                .assess(Some(&version)),
            LocalCliSyntax::NoAdditionalDocumentedFlagFloor
                | LocalCliSyntax::KnownDocumentedSyntax { .. }
        ) {
            return Err(error(
                GithubErrorKind::Unsupported,
                "gh compatibility is outside the reviewed State read syntax",
            ));
        }
        let value = self.json(
            StateSourceRoleV80::Principal,
            vec![
                "api".into(),
                "user".into(),
                "--hostname".into(),
                self.provider.host.clone(),
                "--method".into(),
                "GET".into(),
            ],
        )?;
        let login = value
            .value()
            .get("login")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty() && s.len() <= 100)
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Authentication,
                    "State read lacks actual authenticated login",
                )
            })?;
        validate_name(login)?;
        let id = value
            .value()
            .get("id")
            .and_then(Value::as_u64)
            .filter(|n| *n > 0 && *n <= i64::MAX as u64)
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Authentication,
                    "State read lacks actual positive account",
                )
            })?;
        Ok(HostIdentity {
            host: self.provider.host.clone(),
            login: login.to_ascii_lowercase(),
            account_id: Some(id),
            gh_version: std::str::from_utf8(&raw)
                .map_err(|_| schema())?
                .lines()
                .next()
                .ok_or_else(schema)?
                .trim_end_matches('\r')
                .into(),
        })
    }
    fn open_repositories(&mut self) -> GithubResult<()> {
        let original = Arc::clone(&self.original);
        let StateSourceOriginV80::Catalogue { active, .. } = original.original().origin() else {
            return Err(unavailable());
        };
        let RemoteScopeConsumerV1::Explicit(selected) = active.consumer(&self.identity) else {
            return Err(unavailable());
        };
        if selected.references().len() != original.original().repositories().len()
            || selected.references().is_empty()
            || selected.references().len() > 2
        {
            return Err(unavailable());
        }
        for selected in selected.references() {
            let observed = selected.observed().ok_or_else(unavailable)?;
            validate_repo(&observed.last_name)?;
            if !original
                .original()
                .repositories()
                .contains(&observed.last_name)
            {
                return Err(unavailable());
            }
            let raw = self.json(
                StateSourceRoleV80::Repository,
                vec![
                    "api".into(),
                    format!("repos/{}", observed.last_name),
                    "--hostname".into(),
                    self.provider.host.clone(),
                    "--method".into(),
                    "GET".into(),
                ],
            )?;
            let expected = RepositoryV80 {
                id: observed.id,
                node: observed.node_id.clone(),
                name: observed.last_name.clone(),
            };
            validate_repository(raw.value(), &expected)?;
            if matches!(original.original().origin(), StateSourceOriginV80::Catalogue { source, .. }
                if matches!(source.source().context().source, RepositoryCatalogueSource::PublicUser(_)))
                && raw.value().get("private").and_then(Value::as_bool) != Some(false)
            {
                return Err(unavailable());
            }
            self.repositories.push(expected);
        }
        Ok(())
    }
    fn bind_pool(&mut self, catalogue: bool) -> GithubResult<()> {
        self.heap.check()?;
        if self.pool.is_some() {
            return Err(unavailable());
        }
        let queries = 3usize
            .checked_mul(self.original.original().repositories().len().max(1))
            .ok_or_else(heap_error)?;
        let requests = queries.checked_mul(10).ok_or_else(heap_error)?;
        let closing = PRINCIPAL_CAP
            .checked_add(if catalogue {
                self.repositories
                    .len()
                    .checked_mul(REPOSITORY_CAP)
                    .ok_or_else(heap_error)?
            } else {
                0
            })
            .ok_or_else(heap_error)?;
        let capacity = requests
            .checked_mul(QUEUE_CAP)
            .filter(|n| *n <= self.budget.remaining_bytes().saturating_sub(closing))
            .ok_or_else(heap_error)?;
        let pool = ScopedReadBudgetV1::new(
            self.heap.deadline(),
            capacity,
            requests,
            QUEUE_CAP,
            self.original.original().cancellation(),
        )
        .map_err(GithubError::from)?;
        self.heap.bind_pool(&pool)?;
        self.pool = Some(pool);
        Ok(())
    }
    fn queues(&mut self) -> GithubResult<Inbox> {
        let original = Arc::clone(&self.original);
        let mut strict =
            super::super::inbox_exact_v1::ExactInboxState::state_source_v80(original.original())?;
        let mut records = BTreeMap::<(String, u64), PullRequest>::new();
        let mut coverage = Coverage {
            complete: true,
            ..Coverage::default()
        };
        // No allocated scope list; every actual selected name is visited. An
        // empty actual personal source is one explicit unscoped query family.
        for queue in [Queue::Authored, Queue::Assigned, Queue::ReviewRequested] {
            for index in 0..original.original().repositories().len().max(1) {
                let repository = original
                    .original()
                    .repositories()
                    .get(index)
                    .map(String::as_str);
                let query = match repository {
                    Some(repo) => format!(
                        "is:pr is:open {}:{} repo:{repo}",
                        queue.qualifier(),
                        self.identity.login
                    ),
                    None => format!(
                        "is:pr is:open {}:{}",
                        queue.qualifier(),
                        self.identity.login
                    ),
                };
                strict.begin_query(repository.unwrap_or(""));
                let mut loaded = 0u64;
                for page in 1..=10 {
                    let args = vec![
                        "api".into(),
                        "search/issues".into(),
                        "--hostname".into(),
                        self.provider.host.clone(),
                        "--method".into(),
                        "GET".into(),
                        "-f".into(),
                        format!("q={query}"),
                        "-f".into(),
                        "per_page=100".into(),
                        "-f".into(),
                        format!("page={page}"),
                    ];
                    let raw = self.json(StateSourceRoleV80::Queue, args)?;
                    let result = strict.page(raw.value(), &mut coverage)?;
                    if result.total > 1000 || !coverage.complete {
                        return Err(schema());
                    }
                    let graph = typed_page_bound(raw.value(), result.items.len())?;
                    // Covers all actual output strings, three identity maps,
                    // queue keys, BTree nodes and eventual map->Vec overlap.
                    self.charges.push(self.heap.model(graph)?);
                    for item in result.items {
                        let (mut pr, unique) = strict.state_item_v80(
                            &self.provider,
                            item,
                            &mut coverage,
                            repository,
                        )?;
                        if unique {
                            loaded = loaded.checked_add(1).ok_or_else(schema)?;
                        }
                        pr.queues.insert(queue.clone());
                        let key = (pr.repository.to_ascii_lowercase(), pr.number);
                        if let Some(previous) = records.get_mut(&key) {
                            // Independently queried non-atomic rows may not
                            // silently overwrite a different visible object.
                            if previous.id != pr.id
                                || previous.title != pr.title
                                || previous.body != pr.body
                                || previous.state != pr.state
                                || previous.updated_at != pr.updated_at
                                || previous.repository != pr.repository
                                || previous.url != pr.url
                                || previous.author != pr.author
                                || previous.draft != pr.draft
                                || previous.created_at != pr.created_at
                            {
                                return Err(schema());
                            }
                            previous.queues.insert(queue.clone());
                        } else {
                            records.insert(key, pr);
                        }
                    }
                    strict.check_loaded(loaded, result.total, &mut coverage);
                    if !coverage.complete
                        || loaded > result.total
                        || (result.items.is_empty() && loaded < result.total)
                    {
                        return Err(schema());
                    }
                    if loaded == result.total {
                        break;
                    }
                    if page == 10 {
                        return Err(schema());
                    }
                }
            }
        }
        self.heap.check()?;
        let mut pull_requests: Vec<_> = records.into_values().collect();
        pull_requests.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        coverage.loaded = pull_requests.len();
        Ok(Inbox {
            identity: self.identity.clone(),
            pull_requests,
            coverage,
        })
    }
}
fn validate_repository(raw: &Value, expected: &RepositoryV80) -> GithubResult<()> {
    if raw.get("id").and_then(Value::as_u64) != Some(expected.id)
        || raw.get("full_name").and_then(Value::as_str) != Some(expected.name.as_str())
        || expected
            .node
            .as_deref()
            .is_some_and(|s| raw.get("node_id").and_then(Value::as_str) != Some(s))
        || raw.get("private").and_then(Value::as_bool).is_none()
        || raw.get("permissions").is_some_and(|v| !v.is_object())
    {
        return Err(unavailable());
    }
    Ok(())
}
fn typed_page_bound(raw: &Value, rows: usize) -> GithubResult<usize> {
    fn strings(v: &Value) -> Option<usize> {
        match v {
            Value::String(s) => Some(s.capacity()),
            Value::Array(a) => a.iter().try_fold(0usize, |n, v| n.checked_add(strings(v)?)),
            Value::Object(o) => o.iter().try_fold(0usize, |n, (k, v)| {
                n.checked_add(k.capacity())?.checked_add(strings(v)?)
            }),
            _ => Some(0),
        }
    }
    let large = 32usize
        .checked_add(
            16 * (size_of::<String>()
                + size_of::<u64>()
                + size_of::<usize>()
                + size_of::<PullRequest>()),
        )
        .ok_or_else(heap_error)?;
    let small = 32usize
        .checked_add(16 * (3 * size_of::<String>() + size_of::<u64>() + size_of::<usize>()))
        .ok_or_else(heap_error)?;
    let node = large
        .checked_mul(2)
        .and_then(|n| n.checked_add(small.checked_mul(10)?))
        .and_then(|n| n.checked_add(2 * size_of::<PullRequest>()))
        .ok_or_else(heap_error)?;
    strings(raw)
        .and_then(|n| n.checked_mul(16))
        .and_then(|n| n.checked_add(rows.checked_mul(node)?))
        .and_then(|n| n.checked_add(4096))
        .ok_or_else(heap_error)
}
