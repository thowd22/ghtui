//! Genuine catalogue-source Actions reads, not legacy repository-name fallback.
use crate::{
    actions_workspace::{ActionsAction, ActionsView, ActionsWorkspace, RunPage, ScopedRun},
    active_remote_scope_v1::ActiveRemoteScopeV1,
    providers::{gh::*, runner::CancellationToken},
    repository_catalogue_intent_v1::IntentReference,
    storage::{Cache, CatalogueIntentSource, Scope},
};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

pub const PREPARED_LIMIT_V1: usize = 8 * 1024 * 1024;
pub const BACK_LIMIT_V1: usize = 16 * 1024 * 1024;
pub struct ScopedActionsSourceV1 {
    original: Arc<CatalogueIntentSource>,
    scope: Arc<ActiveRemoteScopeV1>,
    reference: IntentReference,
}
impl ScopedActionsSourceV1 {
    pub fn original(&self) -> &Arc<CatalogueIntentSource> {
        &self.original
    }
    pub fn scope(&self) -> &Arc<ActiveRemoteScopeV1> {
        &self.scope
    }
    pub fn reference(&self) -> &IntentReference {
        &self.reference
    }
}
#[derive(Clone)]
pub enum ScopedActionsPageKindV1 {
    Catalogue {
        runs: Arc<[Arc<ScopedActionsRunTargetV1>]>,
    },
    Attempt {
        run: Arc<ScopedActionsRunTargetV1>,
        jobs: Arc<[Arc<ScopedActionsJobTargetV1>]>,
        model: Arc<RunAttempt>,
    },
}
pub struct ScopedActionsPageV1 {
    source: Arc<ScopedActionsSourceV1>,
    kind: ScopedActionsPageKindV1,
    view: ActionsView,
    selected_job: Option<u64>,
    identity: HostIdentity,
    fetched_at: Option<u64>,
}
impl ScopedActionsPageV1 {
    fn retained_bytes(&self) -> usize {
        let mut bytes = std::mem::size_of::<Self>() + 2 * std::mem::size_of::<usize>();
        bytes = bytes.saturating_add(
            self.identity.host.capacity()
                + self.identity.login.capacity()
                + self.identity.gh_version.capacity(),
        );
        bytes = bytes.saturating_add(
            std::mem::size_of::<ScopedActionsSourceV1>()
                + 2 * std::mem::size_of::<usize>()
                + self.source.reference.last_name.capacity()
                + self
                    .source
                    .reference
                    .node_id
                    .as_ref()
                    .map_or(0, String::capacity),
        );
        match &self.kind {
            ScopedActionsPageKindV1::Catalogue { runs } => {
                bytes = bytes.saturating_add(
                    runs.len() * std::mem::size_of::<Arc<ScopedActionsRunTargetV1>>()
                        + 2 * std::mem::size_of::<usize>(),
                );
                for run in runs.iter() {
                    bytes = bytes.saturating_add(run.owned_bytes_v1());
                }
            }
            ScopedActionsPageKindV1::Attempt { run, jobs, model } => {
                bytes = bytes.saturating_add(run.owned_bytes_v1());
                bytes = bytes.saturating_add(
                    jobs.len() * std::mem::size_of::<Arc<ScopedActionsJobTargetV1>>()
                        + 2 * std::mem::size_of::<usize>(),
                );
                for job in jobs.iter() {
                    bytes = bytes.saturating_add(job.owned_bytes_v1());
                }
                bytes = bytes.saturating_add(
                    crate::actions_workspace::scoped_read_v1::attempt_owned_bytes(model)
                        + 2 * std::mem::size_of::<usize>(),
                );
            }
        }
        bytes
    }
    pub fn source(&self) -> &Arc<ScopedActionsSourceV1> {
        &self.source
    }
    pub fn view(&self) -> ActionsView {
        self.view
    }
    pub fn run(&self) -> Option<&Arc<ScopedActionsRunTargetV1>> {
        match &self.kind {
            ScopedActionsPageKindV1::Attempt { run, .. } => Some(run),
            _ => None,
        }
    }
    pub fn job(&self, id: u64) -> Option<Arc<ScopedActionsJobTargetV1>> {
        match &self.kind {
            ScopedActionsPageKindV1::Attempt { jobs, .. } => {
                jobs.iter().find(|job| job.id() == id).cloned()
            }
            _ => None,
        }
    }
    pub fn run_target(&self, id: u64, attempt: u64) -> Option<Arc<ScopedActionsRunTargetV1>> {
        match &self.kind {
            ScopedActionsPageKindV1::Catalogue { runs } => runs
                .iter()
                .find(|run| run.run().id == id && run.run().attempt == attempt)
                .cloned(),
            ScopedActionsPageKindV1::Attempt { run, .. }
                if run.run().id == id && run.run().attempt == attempt =>
            {
                Some(run.clone())
            }
            _ => None,
        }
    }
}
pub struct InitialScopedActionsV1 {
    pub(crate) source: Arc<ScopedActionsSourceV1>,
    pub(crate) page: Arc<ScopedActionsPageV1>,
    pub(crate) workspace: ActionsWorkspace,
    pub(crate) retained_bytes: usize,
}
/// Worker-only presentation prepared from the actual opaque read BEFORE its
/// owning session's closing repository/SQL/principal proof. No new source or
/// publication authority is created here.
pub(crate) struct PreparedActionsSeedV1 {
    read: ScopedActionsCatalogueReadV1,
    workspace: ActionsWorkspace,
}
pub(crate) fn prepare_initial_read(
    read: ScopedActionsCatalogueReadV1,
    identity: &HostIdentity,
    cancel: &CancellationToken,
) -> Result<PreparedActionsSeedV1, String> {
    let mut workspace = ActionsWorkspace::new(identity.clone(), vec![read.repository().into()]);
    workspace.runs = read
        .catalogue()
        .runs
        .iter()
        .cloned()
        .map(|run| ScopedRun {
            repository: read.repository().into(),
            run,
        })
        .collect();
    workspace.coverage = read.catalogue().coverage.clone();
    workspace.fetched_at = Some(crate::storage::now());
    workspace.prepare_rows();
    crate::actions_workspace::scoped_read_v1::prepare_policy(&mut workspace, false);
    crate::actions_workspace::scoped_read_v1::retained_bytes(&workspace)?;
    if cancel.is_cancelled() {
        return Err("Original Actions seed cancelled".into());
    }
    Ok(PreparedActionsSeedV1 { read, workspace })
}
/// Only the actual provider-read/closed current-access path can call this with
/// genuine observations. No App/JSON/DTO overload exists.
pub(crate) fn initial(
    original: Arc<CatalogueIntentSource>,
    scope: Arc<ActiveRemoteScopeV1>,
    reference: IntentReference,
    seed: PreparedActionsSeedV1,
    closed: &ClosedCatalogueAccess,
    provider: &GithubProvider,
    cancel: &CancellationToken,
) -> Result<InitialScopedActionsV1, String> {
    let PreparedActionsSeedV1 {
        read,
        mut workspace,
    } = seed;
    let current = closed
        .observation(reference.id)
        .ok_or("Original closed Actions repository unavailable")?;
    let scope_matches = matches!(scope.consumer(closed.identity()),crate::active_remote_scope_v1::RemoteScopeConsumerV1::Explicit(refs)
        if refs.references().len()==1 && refs.references()[0].original()==&reference
        && refs.references()[0].observed().is_some_and(|observed|observed.id==read.repository_id()
            && observed.node_id.as_deref()==read.repository_node() && observed.last_name==read.repository()));
    if !read.same_producer(provider)
        || !scope_matches
        || read.repository_id() != reference.id
        || reference
            .node_id
            .as_deref()
            .is_some_and(|node| read.repository_node() != Some(node))
        || current.repository().id != read.repository_id()
        || current.repository().node_id.as_deref() != read.repository_node()
        || current.repository().full_name != read.repository()
        || current.repository().permissions.pull != Some(true)
        || Scope::github(&closed.identity().host, closed.identity().account_id)
            .ok()
            .as_ref()
            != Some(original.scope())
        || crate::remote_scope_gate_v1::stop_before_fallback(Some(&scope)).is_some()
    {
        return Err("Original closed Actions seed unavailable; no DTO promotion".into());
    }
    let (_, runs) = read.into_parts();
    let source = Arc::new(ScopedActionsSourceV1 {
        original,
        scope,
        reference,
    });
    let page = Arc::new(ScopedActionsPageV1 {
        source: source.clone(),
        kind: ScopedActionsPageKindV1::Catalogue { runs },
        view: ActionsView::Runs,
        selected_job: None,
        identity: closed.identity().clone(),
        fetched_at: workspace.fetched_at,
    });
    workspace.identity = closed.identity().clone();
    let retained_bytes = charge(&workspace, &page)?;
    if cancel.is_cancelled() {
        return Err("Original Actions seed cancelled".into());
    }
    Ok(InitialScopedActionsV1 {
        source,
        page,
        workspace,
        retained_bytes,
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopedActionsRequestKindV1 {
    Catalogue,
    Attempt,
    Logs,
    Annotations,
    PreparedView(ActionsView),
}
pub struct ScopedActionsWorkV1 {
    pub(crate) source: Arc<ScopedActionsSourceV1>,
    pub(crate) page: Arc<ScopedActionsPageV1>,
    pub(crate) run: Option<Arc<ScopedActionsRunTargetV1>>,
    pub(crate) job: Option<Arc<ScopedActionsJobTargetV1>>,
    pub(crate) kind: ScopedActionsRequestKindV1,
    pub(crate) deadline: Instant,
    pub(crate) cancellation: CancellationToken,
    pub(crate) epoch: u64,
    #[cfg(test)]
    pub(crate) after_read_fault_v1: Option<fn()>,
}
impl ScopedActionsWorkV1 {
    pub fn host(&self) -> &str {
        &self.source.original.scope().host
    }
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
    pub fn cancellation(&self) -> &CancellationToken {
        &self.cancellation
    }
    pub(crate) fn capture(
        page: Arc<ScopedActionsPageV1>,
        action: &ActionsAction,
        epoch: u64,
    ) -> Result<Arc<Self>, String> {
        let (kind, run, job) = match action {
            ActionsAction::OpenJob(id) => {
                let job = page.job(*id).ok_or("Original selected job unavailable")?;
                (
                    ScopedActionsRequestKindV1::PreparedView(ActionsView::Jobs),
                    Some(job.run().clone()),
                    Some(job),
                )
            }
            ActionsAction::View(view @ (ActionsView::Run | ActionsView::Jobs)) => {
                let run = page.run().ok_or("Original attempt unavailable")?.clone();
                let job = page.selected_job.and_then(|id| page.job(id));
                (
                    ScopedActionsRequestKindV1::PreparedView(*view),
                    Some(run),
                    job,
                )
            }
            ActionsAction::View(ActionsView::Runs) => {
                (ScopedActionsRequestKindV1::Catalogue, None, None)
            }
            ActionsAction::Refresh if page.view == ActionsView::Runs => {
                (ScopedActionsRequestKindV1::Catalogue, None, None)
            }
            ActionsAction::OpenRun {
                repository,
                id,
                attempt,
            } => {
                let run = page
                    .run_target(*id, *attempt)
                    .filter(|run| run.repository() == repository)
                    .ok_or("Selected run is not an original observed catalogue member")?;
                (ScopedActionsRequestKindV1::Attempt, Some(run), None)
            }
            ActionsAction::Refresh => {
                let run = page.run().ok_or("Original attempt unavailable")?.clone();
                let job = page.selected_job.and_then(|id| page.job(id));
                let kind = match page.view {
                    ActionsView::Logs => ScopedActionsRequestKindV1::Logs,
                    ActionsView::Annotations => ScopedActionsRequestKindV1::Annotations,
                    _ => ScopedActionsRequestKindV1::Attempt,
                };
                (kind, Some(run), job)
            }
            ActionsAction::Logs(Some(id)) | ActionsAction::Annotations(id) => {
                let job = page
                    .job(*id)
                    .ok_or("Selected job is not an actual original attempt member")?;
                let kind = if matches!(action, ActionsAction::Annotations(_)) {
                    ScopedActionsRequestKindV1::Annotations
                } else {
                    ScopedActionsRequestKindV1::Logs
                };
                (kind, Some(job.run().clone()), Some(job))
            }
            _ => return Err("This scoped Actions action has no current read protocol".into()),
        };
        Ok(Arc::new(Self {
            source: page.source.clone(),
            page,
            run,
            job,
            kind,
            deadline: Instant::now() + Duration::from_secs(30),
            cancellation: CancellationToken::default(),
            epoch,
            #[cfg(test)]
            after_read_fault_v1: None,
        }))
    }
    pub(crate) fn check(&self) -> Result<Duration, String> {
        if self.cancellation.is_cancelled() {
            return Err("Original Actions read cancelled".into());
        }
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|left| !left.is_zero())
            .ok_or_else(|| "Original Actions read deadline expired".into())
    }
}
pub struct PreparedScopedActionsV1 {
    pub(crate) work: Arc<ScopedActionsWorkV1>,
    pub(crate) page: Arc<ScopedActionsPageV1>,
    pub(crate) workspace: ActionsWorkspace,
    pub(crate) retained_bytes: usize,
}
impl PreparedScopedActionsV1 {
    pub(crate) fn matches(&self, work: &Arc<ScopedActionsWorkV1>) -> bool {
        Arc::ptr_eq(&self.work, work) && Arc::ptr_eq(&self.page.source, &work.source)
    }
}
fn original_guard(original: &CatalogueIntentSource, database: &Path) -> GithubResult<()> {
    let cache = Cache::open(database).map_err(|_| guard_error())?;
    cache
        .validate_repository_catalogue_intent_source(original)
        .map_err(|_| guard_error())
}
fn guard_error() -> GithubError {
    GithubError {
        kind: GithubErrorKind::IdentityChanged,
        message: "Original Actions intent changed; no source rebased".into(),
    }
}
fn charge(workspace: &ActionsWorkspace, page: &ScopedActionsPageV1) -> Result<usize, String> {
    let bytes = crate::actions_workspace::scoped_read_v1::retained_bytes(workspace)?
        .saturating_add(page.retained_bytes());
    if bytes > PREPARED_LIMIT_V1 {
        Err("Scoped Actions workspace/proof capacity exceeds8MiB; no partial complete page".into())
    } else {
        Ok(bytes)
    }
}
fn identity_capacity(identity: &HostIdentity) -> usize {
    identity
        .host
        .capacity()
        .saturating_add(identity.login.capacity())
        .saturating_add(identity.gh_version.capacity())
}
/// Final same-numeric-principal presentation only. Each independently cloned
/// identity is charged by its actual String capacities, never twice another
/// clone's capacities or predicted length. No row/body/SQL work occurs here.
fn replace_final_identity(
    workspace: &mut ActionsWorkspace,
    page: &mut Arc<ScopedActionsPageV1>,
    retained_bytes: &mut usize,
    identity: &HostIdentity,
) -> Result<(), String> {
    let page =
        Arc::get_mut(page).ok_or("Original prepared Actions page was unexpectedly aliased")?;
    let old =
        identity_capacity(&workspace.identity).saturating_add(identity_capacity(&page.identity));
    workspace.identity = identity.clone();
    page.identity = identity.clone();
    let new =
        identity_capacity(&workspace.identity).saturating_add(identity_capacity(&page.identity));
    *retained_bytes = retained_bytes.saturating_sub(old).saturating_add(new);
    if *retained_bytes > PREPARED_LIMIT_V1 {
        return Err("Final identity presentation exceeds original prepared limit".into());
    }
    Ok(())
}

/// Runs in SAME worker and SAME original deadline/token. Errors never expose
/// diagnostics; even a failing inner read closes own repo/SQL/final principal.
pub fn execute(
    work: Arc<ScopedActionsWorkV1>,
    provider: &GithubProvider,
    database: &Path,
) -> Result<PreparedScopedActionsV1, String> {
    let remaining = work.check()?;
    if let ScopedActionsRequestKindV1::PreparedView(view) = work.kind {
        let ScopedActionsPageKindV1::Attempt { model, .. } = &work.page.kind else {
            return Err("Original retained attempt unavailable".into());
        };
        crate::actions_workspace::scoped_read_v1::preflight_projection(model, None)?;
        let mut workspace = ActionsWorkspace::new(
            work.page.identity.clone(),
            vec![work
                .run
                .as_ref()
                .ok_or("Original run unavailable")?
                .repository()
                .into()],
        );
        workspace.run = Some(RunPage {
            repository: workspace.repositories[0].clone(),
            attempt: model.as_ref().clone(),
            selected_job: work.job.as_ref().map(|job| job.id()),
            logs: None,
            annotations: vec![],
        });
        workspace.view = view;
        workspace.fetched_at = work.page.fetched_at;
        workspace.warnings.push("Retained observed attempt • this view adds no current API access or publication permission".into());
        workspace.prepare_rows();
        crate::actions_workspace::scoped_read_v1::focus_selected_job(&mut workspace);
        crate::actions_workspace::scoped_read_v1::prepare_policy(&mut workspace, false);
        let page = Arc::new(ScopedActionsPageV1 {
            source: work.source.clone(),
            kind: work.page.kind.clone(),
            view,
            selected_job: work.job.as_ref().map(|job| job.id()),
            identity: work.page.identity.clone(),
            fetched_at: work.page.fetched_at,
        });
        let retained_bytes = charge(&workspace, &page)?;
        work.check()?;
        return Ok(PreparedScopedActionsV1 {
            work,
            page,
            workspace,
            retained_bytes,
        });
    }
    let bounded = provider
        .clone()
        .with_timeout(remaining)
        .with_scoped_read_window(work.deadline, 512 * 1024)
        .map_err(|_| "Original Actions window unavailable")?;
    let mut limits = RepositoryCatalogueAccessLimits::default();
    limits.transport.max_duration = remaining;
    limits.transport.max_response_bytes = 4 * 1024 * 1024;
    limits.transport.max_document_bytes = 128 * 1024;
    limits.max_references = 1;
    limits.max_repository_reads = 2;
    let mut session = bounded
        .begin_repository_catalogue_access(
            work.source.original.source(),
            &limits,
            &work.cancellation,
        )
        .map_err(|_| "Original Actions principal unavailable")?;
    let window = session
        .reserve_measured_external_reads(24, 512 * 1024, 2 * 1024 * 1024, 2, &work.cancellation)
        .map_err(|_| "Original Actions measured window unavailable")?;
    let scoped = bounded
        .clone()
        .with_measured_read_window(window)
        .map_err(|_| "Original Actions measured window cannot be replaced")?;
    let opening_identity = session.identity().clone();
    let current = session
        .verify(&work.source.reference, &work.cancellation)
        .map_err(|_| "Original Actions repository unavailable")?;
    if current.repository().permissions.pull != Some(true) {
        return Err("Original Actions pull access unavailable; no query fallback".into());
    }
    let canonical = current.repository().full_name.clone();
    enum Inner {
        Catalogue(ScopedActionsCatalogueReadV1),
        Attempt(Box<ScopedActionsAttemptReadV1>),
    }
    let read = match work.kind {
        ScopedActionsRequestKindV1::Catalogue => scoped
            .scoped_actions_catalogue_v1(current, &RunFilter::default(), &work.cancellation)
            .map(Inner::Catalogue),
        kind => {
            let run = work
                .run
                .as_ref()
                .ok_or("Actual original run target unavailable")?
                .clone();
            let read_kind = match kind {
                ScopedActionsRequestKindV1::Logs => ScopedActionsReadKindV1::Logs,
                ScopedActionsRequestKindV1::Annotations => ScopedActionsReadKindV1::Annotations,
                _ => ScopedActionsReadKindV1::Attempt,
            };
            scoped
                .scoped_actions_attempt_v1(
                    current,
                    run,
                    work.job.clone(),
                    read_kind,
                    &work.cancellation,
                    || original_guard(&work.source.original, database),
                )
                .map(|read| Inner::Attempt(Box::new(read)))
        }
    };
    #[cfg(test)]
    if let Some(fault) = work.after_read_fault_v1 {
        fault();
    }
    let prepared = (|| -> Result<(ActionsWorkspace, Arc<ScopedActionsPageV1>, usize), String> {
        let read =
            read.map_err(|_| "Original Actions read unavailable; no private diagnostics retained")?;
        let mut workspace = ActionsWorkspace::new(opening_identity, vec![canonical.clone()]);
        let (kind, view, selected_job) = match read {
            Inner::Catalogue(read) => {
                let (catalog, runs) = read.into_parts();
                workspace.runs = catalog
                    .runs
                    .into_iter()
                    .map(|run| ScopedRun {
                        repository: canonical.clone(),
                        run,
                    })
                    .collect();
                workspace.coverage = catalog.coverage;
                (
                    ScopedActionsPageKindV1::Catalogue { runs },
                    ActionsView::Runs,
                    None,
                )
            }
            Inner::Attempt(read) => {
                let read = *read;
                if read.requested().as_ref().run().id
                    != work.run.as_ref().ok_or("Original run lost")?.run().id
                    || !Arc::ptr_eq(
                        read.requested(),
                        work.run.as_ref().ok_or("Original run lost")?,
                    )
                    || read.selected().map(|job| job.id()) != work.job.as_ref().map(|job| job.id())
                {
                    return Err(
                        "Actual Actions prepared read does not belong to this request".into(),
                    );
                }
                let run = read.target().clone();
                let jobs = read.jobs().clone();
                let (attempt, body) = read.into_parts();
                crate::actions_workspace::scoped_read_v1::preflight_projection(
                    &attempt,
                    Some(&body),
                )?;
                if crate::actions_workspace::scoped_read_v1::attempt_owned_bytes(&attempt)
                    > PREPARED_LIMIT_V1 / 2
                {
                    return Err(
                        "Original Actions model clone would exceed prepared allocation budget"
                            .into(),
                    );
                }
                let model = Arc::new(attempt.clone());
                let selected_job = work.job.as_ref().map(|job| job.id());
                let mut page = RunPage {
                    repository: canonical.clone(),
                    attempt,
                    selected_job,
                    logs: None,
                    annotations: vec![],
                };
                let view = match body {
                    ScopedActionsReadBodyV1::Attempt => {
                        if work.page.view == ActionsView::Jobs {
                            ActionsView::Jobs
                        } else {
                            ActionsView::Run
                        }
                    }
                    ScopedActionsReadBodyV1::Logs(text) => {
                        page.logs = Some(text);
                        ActionsView::Logs
                    }
                    ScopedActionsReadBodyV1::Annotations(rows) => {
                        page.annotations = rows;
                        ActionsView::Annotations
                    }
                    ScopedActionsReadBodyV1::Unavailable(reason) => {
                        workspace.warnings.push(reason.into());
                        ActionsView::Jobs
                    }
                };
                workspace.run = Some(page);
                (
                    ScopedActionsPageKindV1::Attempt { run, jobs, model },
                    view,
                    selected_job,
                )
            }
        };
        workspace.view = view;
        workspace.fetched_at = Some(crate::storage::now());
        workspace.prepare_rows();
        crate::actions_workspace::scoped_read_v1::prepare_labels(&mut workspace);
        crate::actions_workspace::scoped_read_v1::focus_selected_job(&mut workspace);
        crate::actions_workspace::scoped_read_v1::prepare_policy(&mut workspace, false);
        let page = Arc::new(ScopedActionsPageV1 {
            source: work.source.clone(),
            kind,
            view,
            selected_job,
            identity: workspace.identity.clone(),
            fetched_at: workspace.fetched_at,
        });
        let retained_bytes = charge(&workspace, &page)?;
        Ok((workspace, page, retained_bytes))
    })();
    let closing = session
        .verify(&work.source.reference, &work.cancellation)
        .map_err(|_| "Closing original Actions repository unavailable")?;
    if closing.repository().full_name != canonical
        || closing.repository().permissions.pull != Some(true)
    {
        return Err("Original Actions access/name changed during read".into());
    }
    let closed = session
        .close_with_guard(&work.cancellation, || {
            original_guard(&work.source.original, database)
        })
        .map_err(|_| "Original Actions closing source/account proof unavailable")?;
    work.check()?;
    let (mut workspace, mut page, mut retained_bytes) = prepared?;
    // Tiny same-numeric-principal presentation continuity only; no body or
    // SQL after final principal and no foreground re-preparation.
    replace_final_identity(
        &mut workspace,
        &mut page,
        &mut retained_bytes,
        closed.identity(),
    )?;
    work.check()?;
    Ok(PreparedScopedActionsV1 {
        work,
        page,
        workspace,
        retained_bytes,
    })
}
#[cfg(test)]
#[path = "scoped_actions_v1_tests.rs"]
mod tests;
