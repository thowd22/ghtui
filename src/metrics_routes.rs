//! Dashboard worker preparation and verified exact-record routing. Rendering
//! never invokes this module. Current observations are not historical trends.
use crate::{
    app::View,
    dashboard_profile_v1::SavedDashboardV1,
    metrics::{dashboard, MetricTarget, MetricsAction, UtcWindow},
    metrics_repository::{repository_evidence, MetricsReadGuard},
    metrics_workspace::MetricsWorkspace,
    providers::{
        gh::{GithubError, GithubErrorKind, GithubProvider, HostIdentity, MetricsReadLimits},
        runner::CancellationToken,
    },
    runtime::PreparedView,
    storage::{Cache, Scope},
};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub struct MetricsRequest {
    pub declared: Vec<String>,
    pub exclusions: BTreeSet<String>,
    pub previous: Option<MetricsWorkspace>,
    pub action: Option<MetricsAction>,
    pub database: PathBuf,
    pub selected_repository: Option<String>,
    pub window_days: u64,
}
enum Failure {
    Provider(GithubError),
    Invalid(String),
}
impl From<GithubError> for Failure {
    fn from(error: GithubError) -> Self {
        Self::Provider(error)
    }
}
impl From<String> for Failure {
    fn from(error: String) -> Self {
        Self::Invalid(error)
    }
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Provider(error) => error.fmt(f),
            Self::Invalid(error) => error.fmt(f),
        }
    }
}
fn hidden(status: impl Into<String>, clear_private: bool) -> PreparedView {
    PreparedView {
        clear_private,
        clear_metrics: true,
        lines: vec![
            "Dashboard values unavailable • previous counts and record actions hidden".into(),
            "Refresh verifies the explicit repository scope; no mutation is resent.".into(),
        ],
        context: Some("Dashboard • previous values hidden • original storage preserved".into()),
        status: status.into(),
        ..Default::default()
    }
}
fn same_principal(workspace: &MetricsWorkspace, identity: &HostIdentity) -> bool {
    workspace
        .snapshot
        .scope
        .host()
        .eq_ignore_ascii_case(&identity.host)
        && identity.account_id == Some(workspace.snapshot.scope.account_id())
}
fn limits(start: Instant) -> Result<MetricsReadLimits, Failure> {
    let remaining = Duration::from_secs(30)
        .checked_sub(start.elapsed())
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(|| {
            Failure::Invalid("Dashboard source deadline reached; scope is not complete".into())
        })?;
    Ok(MetricsReadLimits {
        max_duration: remaining,
        ..Default::default()
    })
}
fn now() -> Result<u64, Failure> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .map_err(|error| Failure::Invalid(error.to_string()))
}

pub fn prepare(
    mut request: MetricsRequest,
    provider: &GithubProvider,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    // No request may derive an organization, global search, implicit clone or
    // current-directory alias from an absent declaration.
    if request.declared.is_empty() || request.declared.len() > 64 {
        return Ok(hidden(
            "Dashboard scope needed • choose1..64 explicit repositories • no query sent",
            false,
        ));
    }
    for name in &request.declared {
        crate::config::validate_repository(name).map_err(|error| error.to_string())?;
    }
    request.declared = request
        .declared
        .into_iter()
        .map(|name| name.to_ascii_lowercase())
        .filter(|name| {
            !request
                .exclusions
                .iter()
                .any(|excluded| excluded.eq_ignore_ascii_case(name))
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if request.declared.is_empty() {
        return Ok(hidden("Dashboard scope excluded • no query sent", false));
    }
    let identity =
        match provider.identity(cancel) {
            Ok(identity) if identity.account_id.is_some_and(|id| id > 0) => identity,
            Ok(_) => return Ok(hidden(
                "Dashboard account unavailable • previous values hidden • verify gh authentication",
                true,
            )),
            Err(error) => {
                return Ok(hidden(
                    format!("Dashboard account unavailable • previous values hidden: {error}"),
                    true,
                ))
            }
        };
    if request
        .previous
        .as_ref()
        .is_some_and(|previous| !same_principal(previous, &identity))
    {
        return Ok(hidden(
            "Dashboard account changed • previous values hidden",
            true,
        ));
    }
    let action = request.action.take();
    let start = Instant::now();
    let original_declared = request.declared.clone();
    let preference_result = constrain_saved_scope(&mut request, action.as_ref());
    let mut save_after_identity = None;
    let mut restored_profile = None;
    let result = match (preference_result, action.as_ref()) {
        (Err(error), _) => Err(error),
        (Ok(()), Some(MetricsAction::SaveDashboard)) => (|| {
            let previous = request.previous.as_ref().ok_or_else(|| {
                Failure::Invalid("Load Dashboard evidence before saving preferences".into())
            })?;
            let profile = SavedDashboardV1::capture(previous)?;
            let prepared = cached_window(
                &request,
                &identity,
                previous.snapshot.window,
                provider,
                cancel,
                start,
            )?;
            save_after_identity = Some(profile);
            Ok(prepared)
        })(),
        (Ok(()), Some(MetricsAction::RestoreDashboard)) => (|| {
            let scope = Scope::github(&identity.host, identity.account_id)
                .map_err(|error| Failure::Invalid(error.to_string()))?;
            let cache = Cache::open(&request.database)
                .map_err(|error| Failure::Invalid(error.to_string()))?;
            let profile = SavedDashboardV1::load(&cache, &scope, &original_declared)?;
            request.declared = profile.repositories().to_vec();
            request.selected_repository = None;
            request.window_days = profile.window_days();
            let prepared = load(&request, &identity, None, provider, cancel, start)?;
            restored_profile = Some(profile);
            Ok(prepared)
        })(),
        (Ok(()), Some(MetricsAction::DefaultDashboard)) => {
            request.selected_repository = None;
            request.window_days = 7;
            load(&request, &identity, None, provider, cancel, start)
        }
        (Ok(()), Some(MetricsAction::Window(window))) => {
            cached_window(&request, &identity, *window, provider, cancel, start)
        }
        (Ok(()), Some(MetricsAction::Open(target))) => {
            open(&request, &identity, target, provider, cancel, start)
        }
        (Ok(()), other) => load(&request, &identity, other, provider, cancel, start),
    };
    // Even failed repository reads get a current final principal bracket. A
    // failure cannot restore another numeric account's private observations.
    let final_identity = match provider.identity(cancel) {
        Ok(current) if current.account_id.is_some_and(|id| id > 0) => current,
        Ok(_) => {
            return Ok(hidden(
                "Dashboard account unavailable • previous values hidden",
                true,
            ))
        }
        Err(error) => {
            return Ok(hidden(
                format!("Dashboard account unavailable • previous values hidden: {error}"),
                true,
            ))
        }
    };
    if !identity.same_account(&final_identity) {
        return Ok(hidden(
            "Dashboard account changed • previous values hidden",
            true,
        ));
    }
    match result {
        Ok(mut prepared) => {
            if let Some(metrics) = &prepared.metrics {
                if let Some(guard) = &metrics.read_guard {
                    if let Err(error) = guard.validate_parents(&metrics.observations) {
                        return Ok(hidden(
                            format!("Dashboard source changed • previous values hidden: {error}"),
                            false,
                        ));
                    }
                }
            }
            if let Some(metrics) = &mut prepared.metrics {
                if let Some(profile) = restored_profile {
                    metrics
                        .apply_layout(profile.layout().clone())
                        .map_err(|error| error.to_string())?;
                    metrics.restored_scope = Some(Arc::from(profile.repositories().to_vec()));
                    metrics.scope_label = format!("Saved scope • {}", metrics.scope_label);
                    prepared.status = "Saved dashboard restored • fresh repository evidence • preferences are not access authority".into();
                } else if action != Some(MetricsAction::DefaultDashboard) {
                    if let Some(previous) = &request.previous {
                        metrics
                            .apply_layout(previous.layout.clone())
                            .map_err(|error| error.to_string())?;
                        metrics.restored_scope = previous.restored_scope.clone();
                        if metrics.restored_scope.is_some()
                            && !metrics.scope_label.starts_with("Saved scope")
                        {
                            metrics.scope_label = format!("Saved scope • {}", metrics.scope_label);
                        }
                    }
                } else {
                    prepared.status = "Dashboard defaults • all configured repositories • last7 days • all widgets • saved profile preserved".into();
                }
                if let Some(profile) = save_after_identity {
                    if cancel.is_cancelled() || limits(start).is_err() {
                        return Ok(hidden(
                            "Dashboard preference save cancelled or expired • no profile written",
                            false,
                        ));
                    }
                    let saved = (|| -> Result<(), String> {
                        let cache =
                            Cache::open(&request.database).map_err(|error| error.to_string())?;
                        if let Some(guard) = &metrics.read_guard {
                            guard.validate_parents(&metrics.observations)?;
                        }
                        if cancel.is_cancelled() || limits(start).is_err() {
                            return Err(
                                "Dashboard save cancelled or expired before local preference write"
                                    .into(),
                            );
                        }
                        profile.save(&cache)?;
                        if let Some(guard) = &metrics.read_guard {
                            guard.validate_parents(&metrics.observations)?;
                        }
                        Ok(())
                    })();
                    if let Err(error) = saved {
                        return Ok(hidden(
                            format!("Dashboard preferences unavailable: {error}"),
                            false,
                        ));
                    }
                    prepared.status = "Default dashboard saved • account-scoped preferences only • original evidence as-of retained".into();
                }
            }
            prepared.verified_identity = Some(final_identity.clone());
            if let Some(metrics) = prepared
                .metrics
                .as_ref()
                .filter(|_| prepared.navigate_to != Some(View::Inbox))
            {
                prepared.context = Some(format!(
                    "{} / @{} / Dashboard / {}",
                    final_identity.host, final_identity.login, metrics.scope_label
                ));
            }
            Ok(prepared)
        }
        Err(Failure::Provider(error)) => {
            let (label, private) = match error.kind {
                GithubErrorKind::Permission | GithubErrorKind::NotFound => {
                    ("Dashboard access changed • previous values hidden", true)
                }
                GithubErrorKind::IdentityChanged | GithubErrorKind::Authentication => {
                    ("Dashboard identity changed • previous values hidden", true)
                }
                GithubErrorKind::StaleHead => (
                    "Dashboard source changed • previous values hidden • Refresh before opening",
                    false,
                ),
                _ => ("Dashboard unavailable • previous values hidden", false),
            };
            Ok(hidden(format!("{label}: {error}"), private))
        }
        Err(Failure::Invalid(error)) => Ok(hidden(
            format!("Dashboard source changed • previous values hidden: {error}"),
            false,
        )),
    }
}

/// A restored profile is navigation intent, never permission to leave the
/// current explicit set. Only explicit Restore/Defaults replace that intent.
fn constrain_saved_scope(
    request: &mut MetricsRequest,
    action: Option<&MetricsAction>,
) -> Result<(), Failure> {
    if matches!(
        action,
        Some(MetricsAction::RestoreDashboard | MetricsAction::DefaultDashboard)
    ) {
        return Ok(());
    }
    if let Some(scope) = request
        .previous
        .as_ref()
        .and_then(|previous| previous.restored_scope.as_ref())
    {
        if scope.is_empty()
            || scope.len() > 64
            || scope
                .iter()
                .any(|name| !request.declared.iter().any(|declared| declared == name))
        {
            return Err(Failure::Invalid("Saved dashboard scope is no longer declared or is excluded; restore a valid profile or explicitly choose Defaults".into()));
        }
        request.declared = scope.to_vec();
    }
    Ok(())
}

fn load(
    request: &MetricsRequest,
    identity: &HostIdentity,
    action: Option<&MetricsAction>,
    provider: &GithubProvider,
    cancel: &CancellationToken,
    start: Instant,
) -> Result<PreparedView, Failure> {
    let selected = match action {
        Some(MetricsAction::Repository(repository)) => {
            if !request
                .declared
                .iter()
                .any(|name| name.eq_ignore_ascii_case(repository))
            {
                return Err(Failure::Invalid(
                    "Requested repository is outside the declared Dashboard set".into(),
                ));
            }
            vec![repository.to_ascii_lowercase()]
        }
        Some(MetricsAction::AllSelected) => request.declared.clone(),
        Some(MetricsAction::Refresh) | None => match &request.selected_repository {
            Some(repository) if request.declared.iter().any(|name|name.eq_ignore_ascii_case(repository)) => vec![repository.to_ascii_lowercase()],
            Some(_) => return Err(Failure::Invalid("Previous selected repository is outside the current explicit set; choose the scope again".into())),
            None => request.declared.clone(),
        },
        _ => {
            return Err(Failure::Invalid(
                "Dashboard navigation must use its local prepared action semantics".into(),
            ))
        }
    };
    let scope = Scope::github(&identity.host, identity.account_id)
        .map_err(|error| Failure::Invalid(error.to_string()))?;
    let guard = Arc::new(MetricsReadGuard::capture(&request.database, &scope)?);
    let mut inventories = vec![];
    let mut metadata_bytes = 0usize;
    let mut rows = 0usize;
    let mut failed = vec![];
    for name in &selected {
        if cancel.is_cancelled() {
            return Err(Failure::Invalid(
                "Dashboard read cancelled; no mutation sent".into(),
            ));
        }
        let read_limits = match limits(start) {
            Ok(limits) => limits,
            Err(error) => {
                failed.push(error.to_string());
                break;
            }
        };
        match provider.repository_pr_inventory(identity, name, None, &read_limits, cancel) {
            Ok(source) => {
                metadata_bytes = metadata_bytes.saturating_add(source.aggregate_response_bytes);
                rows = rows.saturating_add(source.prs.len());
                if metadata_bytes > 32 * 1024 * 1024 || rows > 10_000 {
                    failed.push("Aggregate Dashboard source rows/bytes budget reached; remaining declared scope is missing".into());
                    break;
                }
                inventories.push(source);
            }
            Err(error)
                if matches!(
                    error.kind,
                    GithubErrorKind::Permission
                        | GithubErrorKind::NotFound
                        | GithubErrorKind::RateLimited
                        | GithubErrorKind::Transport
                ) =>
            {
                failed.push(format!("{name}: {error}"))
            }
            Err(error) => return Err(error.into()),
        }
    }
    let evidence = Arc::new(repository_evidence(identity, &selected, &inventories)?);
    let observations = Arc::from(
        inventories
            .iter()
            .flat_map(|source| source.prs.iter().cloned())
            .collect::<Vec<_>>(),
    );
    guard.validate_parents(&observations)?;
    let as_of = now()?;
    let days = request.window_days;
    if !matches!(days, 7 | 30) {
        return Err(Failure::Invalid(
            "Dashboard requires an explicit7 or30-day UTC window".into(),
        ));
    }
    let window = UtcWindow::new(as_of.saturating_sub(days.saturating_mul(86_400)), as_of)?;
    let snapshot = Arc::new(dashboard(&evidence, as_of, window)?);
    let headers = Arc::from(
        inventories
            .iter()
            .map(|source| source.repository.clone())
            .collect::<Vec<_>>(),
    );
    let mut workspace = MetricsWorkspace::prepare(
        evidence,
        snapshot,
        request.declared.clone(),
        observations,
        headers,
    )?;
    workspace.read_guard = Some(guard.clone());
    if !failed.is_empty() {
        workspace.status = Some(format!("Partial explicit source: {}", failed.join("; ")));
    }
    guard.validate(&request.database)?;
    Ok(PreparedView {
        metrics: Some(workspace),
        status: "Dashboard loaded • repository-wide snapshot • missing history is not zero".into(),
        ..Default::default()
    })
}

fn cached_window(
    request: &MetricsRequest,
    identity: &HostIdentity,
    window: UtcWindow,
    provider: &GithubProvider,
    cancel: &CancellationToken,
    start: Instant,
) -> Result<PreparedView, Failure> {
    let previous = request.previous.as_ref().ok_or_else(|| {
        Failure::Invalid("Load Dashboard evidence before changing its window".into())
    })?;
    if !previous.missing_repositories.is_empty()
        || window.start >= window.end
        || window.end > previous.snapshot.as_of
    {
        return Err(Failure::Invalid(
            "Dashboard window lacks original complete canonical scope/as-of; Refresh first".into(),
        ));
    }
    let guard = previous.read_guard.as_ref().ok_or_else(|| {
        Failure::Invalid("Original Dashboard journal proof unavailable; Refresh first".into())
    })?;
    guard.validate_parents(&previous.observations)?;
    for repository in previous.repositories.iter() {
        provider.metrics_repository_identity(identity, repository, &limits(start)?, cancel)?;
    }
    let snapshot = Arc::new(dashboard(
        &previous.evidence,
        previous.snapshot.as_of,
        window,
    )?);
    let mut workspace = MetricsWorkspace::prepare(
        previous.evidence.clone(),
        snapshot,
        request.declared.clone(),
        previous.observations.clone(),
        previous.repositories.clone(),
    )?;
    for repository in previous.repositories.iter() {
        provider.metrics_repository_identity(identity, repository, &limits(start)?, cancel)?;
    }
    guard.validate_parents(&previous.observations)?;
    workspace.read_guard = Some(guard.clone());
    Ok(PreparedView {
        metrics: Some(workspace),
        status:
            "Dashboard UTC window prepared • original snapshot retained • current access reverified"
                .into(),
        ..Default::default()
    })
}

fn open(
    request: &MetricsRequest,
    identity: &HostIdentity,
    target: &MetricTarget,
    provider: &GithubProvider,
    cancel: &CancellationToken,
    start: Instant,
) -> Result<PreparedView, Failure> {
    let previous = request
        .previous
        .as_ref()
        .ok_or_else(|| Failure::Invalid("Original Dashboard source is unavailable".into()))?;
    target.validate(&previous.snapshot.scope)?;
    let MetricTarget::PullRequest(parent) = target else {
        return Err(Failure::Invalid("This exact metric evidence kind needs its dedicated source viewer; it cannot be substituted with a similarly named PR".into()));
    };
    let original = previous
        .observations
        .iter()
        .find(|source| {
            source
                .repository
                .full_name
                .eq_ignore_ascii_case(&parent.repository)
                && source.number == parent.number
                && Some(source.repository.id) == parent.repository_id
                && Some(source.id) == parent.database_id
                && Some(source.node_id.as_str()) == parent.node_id.as_deref()
        })
        .ok_or_else(|| {
            Failure::Invalid(
                "Original positive canonical metric PR changed; Refresh before opening".into(),
            )
        })?;
    let guard = previous
        .read_guard
        .as_ref()
        .ok_or_else(|| Failure::Invalid("Original Dashboard read proof unavailable".into()))?;
    guard.validate_parents(&previous.observations)?;
    provider.metrics_pr_identity(identity, original, &limits(start)?, cancel)?;
    let github = crate::github_workspace::GithubWorkspace::empty(
        identity.clone(),
        previous
            .snapshot
            .scope
            .repositories()
            .iter()
            .cloned()
            .collect(),
        request.exclusions.clone(),
    )
    .open_metric_pr(original, provider, cancel)?;
    provider.metrics_pr_identity(identity, original, &limits(start)?, cancel)?;
    guard.validate_parents(&previous.observations)?;
    let mut retained = previous.clone();
    retained.pending = None;
    retained.status = None;
    Ok(PreparedView {
        context: Some(github.context()),
        status: "Opened exact metric PR • verified original IDs/head • no Git/GitHub changes"
            .into(),
        metrics: Some(retained),
        github: Some(github),
        github_origin: Some(View::Dashboard),
        navigate_to: Some(View::Inbox),
        ..Default::default()
    })
}
