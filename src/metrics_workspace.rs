//! Prepared Dashboard navigation, shared by native mouse and keyboard actions.
//! Workers authorize inputs and build this model; drawing never calculates metrics or reads IO.
use crate::{
    config::validate_repository,
    identity::ActorIdentity,
    metrics::{
        Contributor, CoverageState, DashboardEvidence, DashboardSnapshot, EvidenceCoverage, Health,
        MetricKind, MetricTarget, MetricValue, MetricsAction, UtcWindow,
    },
    providers::{
        gh::{MetricsPrObservation, MetricsRepository},
        runner::sanitize_terminal,
    },
    theme::SemanticToken,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use unicode_segmentation::UnicodeSegmentation;

const WRAP_CELLS: usize = 68;
const MAX_PREPARED_ROWS: usize = 250_000;
const MAX_METADATA_BYTES: usize = 32 * 1024 * 1024;
const MAX_FIELD_BYTES: usize = 256 * 1024;
const MAX_ROW_BYTES: usize = 4096;

#[derive(Default)]
struct PreparationBudget {
    bytes: usize,
}
impl PreparationBudget {
    fn text(&mut self, text: &str) -> Result<(), String> {
        self.bytes = self.bytes.saturating_add(text.len());
        if text.len() > MAX_FIELD_BYTES || self.bytes > MAX_METADATA_BYTES {
            return Err("Dashboard raw metadata byte budget exceeded before preparation".into());
        }
        Ok(())
    }
    fn coverage(&mut self, coverage: &EvidenceCoverage) -> Result<(), String> {
        match &coverage.state {
            CoverageState::Complete => {}
            CoverageState::Unavailable(reason) => self.text(reason)?,
            CoverageState::Partial(reasons) => {
                if reasons.len() > 256 {
                    return Err("Dashboard source reason budget exceeded".into());
                }
                for reason in reasons {
                    self.text(reason)?;
                }
            }
        }
        Ok(())
    }
    fn target(&mut self, target: &MetricTarget) -> Result<(), String> {
        if let Some(pr) = parent_of(target) {
            self.text(&pr.repository)?;
            if let Some(node) = &pr.node_id {
                self.text(node)?;
            }
            if let Some(raw) = &pr.source_record_id {
                self.text(raw)?;
            }
        }
        match target {
            MetricTarget::Check { head_sha, kind, .. } => {
                self.text(head_sha)?;
                self.text(kind)?;
            }
            MetricTarget::Run {
                repository,
                head_sha,
                ..
            } => {
                self.text(repository)?;
                self.text(head_sha)?;
            }
            _ => {}
        }
        Ok(())
    }
}
fn preflight(
    evidence: &DashboardEvidence,
    snapshot: &DashboardSnapshot,
    declared: &[String],
    observations: &[MetricsPrObservation],
    repositories: &[MetricsRepository],
) -> Result<(), String> {
    if declared.len() > 64
        || repositories.len() > 64
        || observations.len() > 10_000
        || snapshot.cards.len() > 8
        || snapshot.notices.len() > 256
    {
        return Err("Dashboard source collection budget exceeded".into());
    }
    let mut budget = PreparationBudget::default();
    for repository in declared {
        budget.text(repository)?;
    }
    for repository in repositories {
        for field in [&repository.host, &repository.full_name, &repository.node_id] {
            budget.text(field)?;
        }
    }
    for source in observations {
        budget.text(&source.node_id)?;
        budget.text(&source.repository.full_name)?;
        budget.text(&source.repository.node_id)?;
    }
    for coverage in [
        &evidence.pr_coverage,
        &evidence.check_coverage,
        &evidence.run_coverage,
    ] {
        budget.coverage(coverage)?;
    }
    for notice in &snapshot.notices {
        budget.text(notice)?;
    }
    for card in &snapshot.cards {
        budget.text(&card.label)?;
        budget.text(&card.formula)?;
        budget.text(&card.cohort)?;
        budget.coverage(&card.coverage)?;
        if card.contributors.len() > 50_000 {
            return Err("Dashboard contributor count budget exceeded".into());
        }
        for contributor in &card.contributors {
            budget.text(&contributor.title)?;
            budget.text(&contributor.note)?;
            budget.target(&contributor.target)?;
            if contributor.actors.len() > 100 || contributor.evidence.len() > 1000 {
                return Err("Dashboard contributor actor/evidence budget exceeded".into());
            }
            for actor in &contributor.actors {
                budget.text(&actor.actor.login)?;
                budget.text(&actor.actor.actor_type)?;
                if let Some(name) = &actor.actor.display_name {
                    budget.text(name)?;
                }
                if let Some(url) = &actor.actor.avatar_url {
                    budget.text(url)?;
                }
            }
            for target in &contributor.evidence {
                budget.target(target)?;
            }
        }
    }
    Ok(())
}
fn prepared_budget(
    rows: &[MetricsPreparedRow],
    total_rows: &mut usize,
    total_bytes: &mut usize,
) -> Result<(), String> {
    *total_rows = total_rows.saturating_add(rows.len());
    for row in rows {
        *total_bytes = total_bytes.saturating_add(row.label.len());
        if row.label.len() > MAX_ROW_BYTES {
            return Err("Dashboard prepared row byte budget exceeded".into());
        }
    }
    if *total_rows > MAX_PREPARED_ROWS || *total_bytes > MAX_METADATA_BYTES {
        return Err("Dashboard prepared rows/bytes budget exceeded".into());
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MetricsWorkspaceAction {
    Layout,
    ToggleWidget(usize),
    MoveWidget(bool),
    Metric(MetricsAction),
    Select(usize),
    Inspect(usize),
    Repositories,
    /// Open the retained daily trend of the card at this row (or the card on screen).
    Trend(usize),
    /// Inspect one retained trend day's records (index into that trend's retained days).
    TrendDay(usize),
    /// Open the stale open PRs widget (derived from the loaded inventory, no read).
    Stale,
    /// List the account's saved named dashboards.
    Dashboards,
    /// Start naming a new dashboard saved from the current one.
    NewDashboard,
    /// Start renaming the selected saved dashboard.
    RenameDashboard,
    /// Arm, then confirm, deleting the selected saved dashboard.
    DeleteDashboard,
    /// Name entry: one typed character, Backspace, submit or discard.
    NameChar(char),
    NameBackspace,
    NameSubmit,
    NameCancel,
    Explain(String),
}
/// A dashboard name being typed: for a new dashboard, or to rename `rename`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameEntry {
    pub rename: Option<String>,
    pub text: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetricsView {
    Layout,
    Cards,
    Contributors(usize),
    Details {
        card: usize,
        contributor: usize,
    },
    CardInfo(usize),
    Repositories,
    Trend(usize),
    TrendDay {
        card: usize,
        day: usize,
    },
    /// Open PRs whose GitHub `updated_at` is at least [`STALE_DAYS`] before the as-of.
    Stale,
    /// Unresolved review threads read on demand for the loaded open PRs.
    Threads,
    /// The account's saved named dashboards.
    Dashboards,
}
/// The default stale threshold: an open PR is stale when GitHub last updated it at least
/// this many days before the as-of. Each dashboard may choose another.
pub const STALE_DAYS: u64 = crate::dashboard_profile_v1::DEFAULT_STALE_DAYS;
#[derive(Clone, Debug)]
pub struct MetricsPreparedRow {
    pub label: String,
    pub role: SemanticToken,
    pub actor: Option<ActorIdentity>,
    pub avatar_only: bool,
    pub action: Option<MetricsWorkspaceAction>,
    /// Table cells for the Cards ([`CARD_COLUMNS`]) and Contributors
    /// ([`CONTRIBUTOR_COLUMNS`]) lists; empty for prose rows, which show `label`.
    pub cells: Vec<crate::ui_table::Cell>,
}
/// Dashboard cards: the metric name, then its value.
pub const CARD_COLUMNS: [crate::ui_table::Column; 2] = [
    crate::ui_table::Column::new("METRIC", 12).priority(0),
    crate::ui_table::Column::new("VALUE", 8).flex(1).priority(1),
];
/// One card's contributors: the PR or run, its title, the first actor and the measurement.
pub const CONTRIBUTOR_COLUMNS: [crate::ui_table::Column; 4] = [
    crate::ui_table::Column::new("ITEM", 8).flex(1).priority(1),
    crate::ui_table::Column::new("TITLE", 10)
        .flex(3)
        .priority(0),
    crate::ui_table::Column::new("ACTOR", 6).priority(3).actor(),
    crate::ui_table::Column::new("MEASURE", 4)
        .priority(2)
        .right(),
];
#[derive(Clone, Debug)]
pub struct MetricsPreparedCard {
    pub title: String,
    pub summary: String,
    pub coverage: String,
    pub formula_rows: Arc<[MetricsPreparedRow]>,
    pub contributors: Arc<[MetricsPreparedRow]>,
    pub details: Arc<[Arc<[MetricsPreparedRow]>]>,
}
#[derive(Clone, Debug)]
pub struct MetricsWorkspace {
    pub layout: crate::dashboard_profile_v1::DashboardLayoutV1,
    /// Restored preference scope only; fresh reads must still prove access.
    pub restored_scope: Option<Arc<[String]>>,
    all_card_rows: Arc<[MetricsPreparedRow]>,
    layout_rows: Arc<[MetricsPreparedRow]>,
    layout_return: (MetricsView, usize, usize),
    pub evidence: Arc<DashboardEvidence>,
    pub snapshot: Arc<DashboardSnapshot>,
    /// Immutable genuine source records for Root's fresh canonical parent-opening worker.
    pub observations: Arc<[MetricsPrObservation]>,
    pub repositories: Arc<[MetricsRepository]>,
    pub missing_repositories: Arc<[String]>,
    pub read_guard: Option<Arc<crate::metrics_repository::MetricsReadGuard>>,
    pub declared_repositories: Arc<[String]>,
    pub scope_label: String,
    pub cohort_label: String,
    pub window_label: String,
    pub as_of_label: String,
    pub source_coverage_label: String,
    pub card_rows: Arc<[MetricsPreparedRow]>,
    pub cards: Arc<[MetricsPreparedCard]>,
    pub repository_rows: Arc<[MetricsPreparedRow]>,
    pub view: MetricsView,
    pub selected: usize,
    pub top: usize,
    /// Pending requests suppress old counts, contributor rows and Open immediately.
    pub pending: Option<MetricsAction>,
    pub status: Option<String>,
    card_position: (usize, usize),
    contributor_positions: Vec<(usize, usize)>,
    repository_return: (MetricsView, usize, usize),
    info_return: (MetricsView, usize, usize),
    canonical_index: Arc<BTreeMap<(String, u64), usize>>,
    /// Retained daily trends per snapshot card; empty until the worker attaches them.
    pub trends: Arc<[crate::metrics_trend_v1::PreparedTrend]>,
    trend_return: (MetricsView, usize, usize),
    trend_day_return: (usize, usize),
    /// Stale open PRs, oldest update first; each row opens its exact PR.
    pub stale_rows: Arc<[MetricsPreparedRow]>,
    pub stale_summary: String,
    stale_return: (MetricsView, usize, usize),
    /// Unresolved review threads; empty until the worker attaches a read.
    pub thread_rows: Arc<[MetricsPreparedRow]>,
    pub thread_summary: Option<String>,
    threads_return: (MetricsView, usize, usize),
    /// This dashboard's stale threshold in days (one of `STALE_THRESHOLDS`).
    pub stale_days: u64,
    /// The saved named dashboard this one was opened from or saved as.
    pub active_dashboard: Option<String>,
    /// The account's saved dashboards as last read by the worker.
    pub saved_dashboards: Arc<[crate::dashboard_profile_v1::DashboardEntry]>,
    /// Why the saved dashboards could not be listed, when they could not.
    pub saved_dashboards_note: Option<String>,
    dashboard_rows: Arc<[MetricsPreparedRow]>,
    dashboards_return: (MetricsView, usize, usize),
    pub name_entry: Option<NameEntry>,
    /// The saved dashboard a first Delete armed; a second Delete confirms it.
    pub delete_armed: Option<String>,
}
fn row(
    label: String,
    role: SemanticToken,
    action: Option<MetricsWorkspaceAction>,
) -> MetricsPreparedRow {
    MetricsPreparedRow {
        label,
        role,
        actor: None,
        avatar_only: false,
        action,
        cells: Vec::new(),
    }
}
pub fn coverage_label(coverage: &EvidenceCoverage) -> String {
    match &coverage.state {
        CoverageState::Complete => "Complete observed source".into(),
        CoverageState::Partial(reasons) => format!("Partial: {}", reasons.join("; ")),
        CoverageState::Unavailable(reason) => format!("Unavailable: {reason}"),
    }
}
fn coverage_role(coverage: &EvidenceCoverage) -> SemanticToken {
    match coverage.state {
        CoverageState::Complete => SemanticToken::Success,
        _ => SemanticToken::Pending,
    }
}
pub(crate) fn value_label(value: &MetricValue) -> String {
    match value {
        MetricValue::Count(count) => count.to_string(),
        MetricValue::MeanSeconds {sum, samples} if *samples > 0 => format!("{}s mean / {samples} measured", sum / u128::from(*samples)),
        MetricValue::HealthCounts {success, failure, pending, action_required, cancelled, neutral, unknown} => format!("Passed {success}, Failed {failure}, Pending {pending}, Action {action_required}, Cancelled {cancelled}, Neutral {neutral}, Unknown {unknown}"),
        _ => "Unavailable (not zero)".into(),
    }
}
fn health_label(health: Health) -> (&'static str, SemanticToken) {
    match health {
        Health::Success => ("Passed", SemanticToken::Success),
        Health::Failure => ("Failed", SemanticToken::Failure),
        Health::Pending => ("Pending", SemanticToken::Pending),
        Health::ActionRequired => ("Action required", SemanticToken::Pending),
        Health::Cancelled => ("Cancelled", SemanticToken::Cancelled),
        Health::Neutral => ("Neutral", SemanticToken::Muted),
        Health::Unknown => ("Unknown", SemanticToken::Muted),
    }
}
/// UTC calendar display from the supplied immutable clock; never wall-clock IO in draw.
pub fn utc_label(seconds: u64) -> String {
    if seconds > 253_402_300_799 {
        return format!("Unix {seconds} UTC (calendar unavailable)");
    }
    let mut days = seconds / 86_400;
    let mut year = 1970u64;
    let leap = |year: u64| {
        year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
    };
    loop {
        let length = if leap(year) { 366 } else { 365 };
        if days < length {
            break;
        }
        days -= length;
        year += 1;
    }
    let months = [
        31,
        if leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 0;
    while days >= months[month] {
        days -= months[month];
        month += 1;
    }
    let clock = seconds % 86_400;
    format!(
        "{year:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
        month + 1,
        days + 1,
        clock / 3600,
        clock / 60 % 60,
        clock % 60
    )
}
fn wrapped(text: &str, role: SemanticToken) -> Result<Vec<MetricsPreparedRow>, String> {
    if text.len() > MAX_FIELD_BYTES {
        return Err("Dashboard field byte budget exceeded before wrapping".into());
    }
    let safe = sanitize_terminal(text);
    if safe.len() > MAX_FIELD_BYTES {
        return Err("Dashboard sanitized field byte budget exceeded".into());
    }
    let mut output = Vec::new();
    for line in safe.lines() {
        let mut chunk = String::new();
        let mut cells = 0usize;
        let mut last_break = None;
        for grapheme in line.graphemes(true) {
            if grapheme.len() > MAX_ROW_BYTES {
                return Err("Dashboard source grapheme byte budget exceeded".into());
            }
            let width = crate::editor::grapheme_width(grapheme);
            while (cells.saturating_add(width) > WRAP_CELLS
                || chunk.len().saturating_add(grapheme.len()) > MAX_ROW_BYTES)
                && !chunk.is_empty()
            {
                if let Some((byte, break_cells)) = last_break.take() {
                    let tail = chunk.split_off(byte);
                    output.push(row(chunk.trim_end().to_owned(), role, None));
                    chunk = tail;
                    cells = cells.saturating_sub(break_cells);
                } else {
                    output.push(row(std::mem::take(&mut chunk), role, None));
                    cells = 0;
                }
            }
            chunk.push_str(grapheme);
            cells = cells.saturating_add(width);
            if grapheme.chars().all(char::is_whitespace) {
                last_break = Some((chunk.len(), cells));
            }
        }
        output.push(row(chunk, role, None));
        if output.len() > MAX_PREPARED_ROWS {
            return Err("Dashboard wrapped row budget exceeded".into());
        }
    }
    Ok(output)
}
fn parent_of(target: &MetricTarget) -> Option<&crate::metrics::PrTarget> {
    match target {
        MetricTarget::PullRequest(pr)
        | MetricTarget::Review { pr, .. }
        | MetricTarget::Event { pr, .. }
        | MetricTarget::Check { pr, .. } => Some(pr),
        MetricTarget::Run { .. } => None,
    }
}
fn canonical_identity<'a>(
    snapshot: &DashboardSnapshot,
    observations: &'a [MetricsPrObservation],
    index: &BTreeMap<(String, u64), usize>,
    target: &MetricTarget,
) -> Result<Option<&'a MetricsPrObservation>, String> {
    target.validate(&snapshot.scope)?;
    if let Some(pr) = parent_of(target) {
        if !pr.has_canonical_identity() {
            return Err(
                "Canonical PR identity unavailable; a number/raw ID cannot authorize Open".into(),
            );
        }
        let original = index
            .get(&(pr.repository.to_ascii_lowercase(), pr.number))
            .and_then(|position| observations.get(*position))
            .filter(|source| {
                source
                    .repository
                    .host
                    .eq_ignore_ascii_case(snapshot.scope.host())
                    && Some(source.repository.id) == pr.repository_id
                    && Some(source.id) == pr.database_id
                    && Some(source.node_id.as_str()) == pr.node_id.as_deref()
            })
            .ok_or("Original genuine canonical PR observation unavailable; refresh before Open")?;
        if let MetricTarget::Check { head_sha, .. } = target {
            if original
                .head_sha
                .as_ref()
                .is_none_or(|head| !head.eq_ignore_ascii_case(head_sha))
            {
                return Err("Check does not identify the original exact current PR head".into());
            }
        }
        return Ok(Some(original));
    } else if matches!(
        target,
        MetricTarget::Run {
            repository_id: None,
            ..
        }
    ) {
        return Err("Canonical run repository identity unavailable; refresh before Open".into());
    }
    Ok(None)
}
fn canonical_open(
    snapshot: &DashboardSnapshot,
    observations: &[MetricsPrObservation],
    index: &BTreeMap<(String, u64), usize>,
    target: &MetricTarget,
) -> Result<(), String> {
    if canonical_identity(snapshot, observations, index, target)?.is_some_and(|source| {
        source
            .head_sha
            .as_ref()
            .is_none_or(|head| !crate::search::model::valid_sha(head))
    }) {
        return Err(
            "Original full PR head unavailable; refresh rather than guess a native destination"
                .into(),
        );
    }
    Ok(())
}
fn open_action(
    snapshot: &DashboardSnapshot,
    observations: &[MetricsPrObservation],
    index: &BTreeMap<(String, u64), usize>,
    target: &MetricTarget,
) -> MetricsWorkspaceAction {
    canonical_open(snapshot, observations, index, target)
        .map_or_else(MetricsWorkspaceAction::Explain, |_| {
            MetricsWorkspaceAction::Metric(MetricsAction::Open(target.clone()))
        })
}
pub(crate) fn target_label(target: &MetricTarget) -> String {
    match target {
        MetricTarget::PullRequest(pr) => format!("{}#{}", pr.repository, pr.number),
        MetricTarget::Review { pr, review_id } => {
            format!("{}#{} review {review_id}", pr.repository, pr.number)
        }
        MetricTarget::Event { pr, event_id } => {
            format!("{}#{} event {event_id}", pr.repository, pr.number)
        }
        MetricTarget::Check {
            pr, check_id, kind, ..
        } => format!("{}#{} {kind} {check_id}", pr.repository, pr.number),
        MetricTarget::Run {
            repository,
            run_id,
            attempt,
            ..
        } => format!("{repository} run {run_id} attempt {attempt}"),
    }
}
fn contributor_details(
    snapshot: &DashboardSnapshot,
    observations: &[MetricsPrObservation],
    index: &BTreeMap<(String, u64), usize>,
    contributor: &Contributor,
) -> Result<Vec<MetricsPreparedRow>, String> {
    let mut output = wrapped(&contributor.title, SemanticToken::Head)?;
    let caption = target_label(&contributor.target);
    output.push(row(
        format!("[Enter Open {caption}]"),
        SemanticToken::Reference,
        Some(open_action(
            snapshot,
            observations,
            index,
            &contributor.target,
        )),
    ));
    if let Some(pr) = parent_of(&contributor.target) {
        for label in [
            format!(
                "Repository ID: {}",
                pr.repository_id
                    .map_or_else(|| "Unavailable".into(), |id| id.to_string())
            ),
            format!(
                "PR ID: {}",
                pr.database_id
                    .map_or_else(|| "Unavailable".into(), |id| id.to_string())
            ),
            format!(
                "Genuine PR node: {}",
                pr.node_id.as_deref().unwrap_or("Unavailable")
            ),
        ] {
            output.extend(wrapped(&label, SemanticToken::Reference)?);
        }
        if let Some(raw) = &pr.source_record_id {
            output.extend(wrapped(
                &format!("Legacy record ID (not canonical authority): {raw}"),
                SemanticToken::Muted,
            )?);
        }
    }
    if let MetricTarget::Check { head_sha, .. } | MetricTarget::Run { head_sha, .. } =
        &contributor.target
    {
        output.extend(wrapped(
            &format!("Exact head {head_sha}"),
            SemanticToken::Reference,
        )?);
    }
    if let Some(seconds) = contributor.measured_seconds {
        output.extend(wrapped(
            &format!("Measured {seconds} seconds"),
            SemanticToken::Normal,
        )?);
    }
    if let Some(health) = contributor.health {
        let (label, role) = health_label(health);
        output.push(row(label.into(), role, None));
    }
    for actor in &contributor.actors {
        let identity = actor.identity(&snapshot.scope);
        output.extend(wrapped(
            &format!(
                "Actor source numeric ID: {}",
                actor
                    .account_id
                    .map_or_else(|| "Unavailable".into(), |id| id.to_string())
            ),
            SemanticToken::Actor,
        )?);
        output.push(MetricsPreparedRow {
            label: identity.label(),
            role: SemanticToken::Actor,
            actor: Some(identity),
            avatar_only: false,
            action: None,
            cells: Vec::new(),
        });
    }
    for target in &contributor.evidence {
        let caption = target_label(target);
        output.push(row(
            format!("[Enter Open evidence {caption}]"),
            SemanticToken::Reference,
            Some(open_action(snapshot, observations, index, target)),
        ));
    }
    if !contributor.note.is_empty() {
        output.extend(wrapped(&contributor.note, SemanticToken::Muted)?);
    }
    Ok(output)
}
impl MetricsWorkspace {
    /// Invoke only in the worker after fresh original-principal/resource/read-guard validation.
    pub fn prepare(
        evidence: Arc<DashboardEvidence>,
        snapshot: Arc<DashboardSnapshot>,
        declared_repositories: Vec<String>,
        observations: Arc<[MetricsPrObservation]>,
        repositories: Arc<[MetricsRepository]>,
    ) -> Result<Self, String> {
        evidence.scope.validate()?;
        preflight(
            &evidence,
            &snapshot,
            &declared_repositories,
            &observations,
            &repositories,
        )?;
        if evidence.scope != snapshot.scope
            || snapshot.window.start >= snapshot.window.end
            || snapshot.window.end > snapshot.as_of
        {
            return Err(
                "Dashboard snapshot/evidence scope or immutable UTC interval differs".into(),
            );
        }
        let mut repository_names = BTreeSet::new();
        let mut repository_nodes = BTreeSet::new();
        let mut repository_ids = BTreeSet::new();
        for repository in repositories.iter() {
            if repository.id == 0
                || repository.node_id.is_empty()
                || repository.node_id.chars().any(char::is_control)
                || !repository.host.eq_ignore_ascii_case(snapshot.scope.host())
                || !repository_names.insert(repository.full_name.to_ascii_lowercase())
                || !repository_nodes.insert(repository.node_id.clone())
                || !repository_ids.insert(repository.id)
            {
                return Err(
                    "Dashboard canonical repository headers are foreign, missing or conflicting"
                        .into(),
                );
            }
        }
        if !repository_names.is_subset(snapshot.scope.repositories())
            || (&repository_names != snapshot.scope.repositories()
                && matches!(evidence.pr_coverage.state, CoverageState::Complete))
        {
            return Err("Dashboard canonical repository headers must exactly cover the explicit source scope, including empty inventories".into());
        }
        let missing_repositories = snapshot
            .scope
            .repositories()
            .difference(&repository_names)
            .cloned()
            .collect::<Vec<_>>();
        let mut declared = BTreeSet::new();
        for repository in declared_repositories {
            validate_repository(&repository).map_err(|error| error.to_string())?;
            declared.insert(repository.to_ascii_lowercase());
        }
        if declared.is_empty()
            || declared.len() > 64
            || !snapshot.scope.repositories().is_subset(&declared)
            || (snapshot.scope.repositories() != &declared
                && snapshot.scope.repositories().len() != 1)
        {
            return Err(
                "Dashboard scope is all declared repositories or exactly one declared repository"
                    .into(),
            );
        }
        let mut canonical = BTreeMap::new();
        for (position, observation) in observations.iter().enumerate() {
            if observation.repository.id == 0
                || observation.id == 0
                || observation.number == 0
                || observation.repository.node_id.is_empty()
                || observation.node_id.is_empty()
                || !observation
                    .repository
                    .host
                    .eq_ignore_ascii_case(snapshot.scope.host())
                || !snapshot.scope.accepts(&observation.repository.full_name)
                || !repositories
                    .iter()
                    .any(|repository| repository == &observation.repository)
            {
                return Err(
                    "Dashboard original observations have foreign or missing canonical identity"
                        .into(),
                );
            }
            let key = (
                observation.repository.full_name.to_ascii_lowercase(),
                observation.number,
            );
            if canonical
                .insert(key, position)
                .is_some_and(|old| &observations[old] != observation)
            {
                return Err("Conflicting original canonical PR observations".into());
            }
        }
        let all_kinds = [
            MetricKind::OpenPrs,
            MetricKind::NeedsMyReview,
            MetricKind::OpenAge,
            MetricKind::ReviewWait,
            MetricKind::FirstReview,
            MetricKind::MergeLatency,
            MetricKind::Checks,
            MetricKind::Workflows,
        ];
        if snapshot.cards.len() != all_kinds.len()
            || all_kinds.iter().any(|kind| {
                snapshot
                    .cards
                    .iter()
                    .filter(|card| card.kind == *kind)
                    .count()
                    != 1
            })
        {
            return Err("Dashboard requires exactly the eight declared metric cards".into());
        }
        let mut cards = Vec::new();
        let mut card_rows = Vec::new();
        let mut bytes = 0usize;
        let mut total_rows = 0usize;
        let mut prepared_bytes = 0usize;
        for card in &snapshot.cards {
            bytes = bytes.saturating_add(card.label.len() + card.formula.len() + card.cohort.len());
            let title = sanitize_terminal(&card.label);
            let summary = sanitize_terminal(&format!(
                "{} • Unknown {} • Censored {}",
                value_label(&card.value),
                card.unknown,
                card.censored
            ));
            let coverage = sanitize_terminal(&coverage_label(&card.coverage));
            let mut formula_rows = wrapped(&summary, SemanticToken::Normal)?;
            formula_rows.extend(wrapped(&card.formula, SemanticToken::Muted)?);
            formula_rows.extend(wrapped(&card.cohort, SemanticToken::Muted)?);
            formula_rows.extend(wrapped(&coverage, coverage_role(&card.coverage))?);
            prepared_budget(&formula_rows, &mut total_rows, &mut prepared_bytes)?;
            let mut contributors = Vec::new();
            let mut details = Vec::new();
            if card.contributors.len() > 50_000 {
                return Err("Dashboard contributor row budget exceeded".into());
            }
            for contributor in &card.contributors {
                if parent_of(&contributor.target).is_some_and(|pr| pr.has_canonical_identity()) {
                    canonical_identity(&snapshot, &observations, &canonical, &contributor.target)?;
                }
                for target in &contributor.evidence {
                    if parent_of(target).is_some_and(|pr| pr.has_canonical_identity()) {
                        canonical_identity(&snapshot, &observations, &canonical, target)?;
                    }
                }
                bytes = bytes.saturating_add(contributor.title.len() + contributor.note.len());
                for actor in &contributor.actors {
                    bytes = bytes.saturating_add(
                        actor.actor.login.len()
                            + actor.actor.actor_type.len()
                            + actor.actor.display_name.as_ref().map_or(0, String::len)
                            + actor.actor.avatar_url.as_ref().map_or(0, String::len),
                    );
                }
                if bytes > MAX_METADATA_BYTES {
                    return Err("Dashboard prepared metadata byte budget exceeded".into());
                }
                let (measurement, role) = contributor.health.map_or_else(
                    || {
                        (
                            contributor.measured_seconds.map_or(
                                if matches!(
                                    card.kind,
                                    MetricKind::OpenPrs | MetricKind::NeedsMyReview
                                ) {
                                    "Current PR record".into()
                                } else {
                                    "No measured latency; inspect note".into()
                                },
                                |seconds| format!("{seconds}s"),
                            ),
                            SemanticToken::Normal,
                        )
                    },
                    |health| {
                        let (label, role) = health_label(health);
                        (label.to_owned(), role)
                    },
                );
                let actor_labels = contributor
                    .actors
                    .iter()
                    .map(|actor| actor.identity(&snapshot.scope).label())
                    .collect::<Vec<_>>()
                    .join(" ");
                let mut contributor_row = row(
                    sanitize_terminal(&format!(
                        "{actor_labels} • {} • {} • {measurement}",
                        target_label(&contributor.target),
                        contributor.title
                    )),
                    role,
                    Some(open_action(
                        &snapshot,
                        &observations,
                        &canonical,
                        &contributor.target,
                    )),
                );
                contributor_row.actor = contributor
                    .actors
                    .first()
                    .map(|actor| actor.identity(&snapshot.scope));
                contributor_row.avatar_only = true;
                contributor_row.cells = vec![
                    crate::ui_table::Cell::styled(
                        sanitize_terminal(&target_label(&contributor.target)),
                        crate::theme::Role::TextMuted,
                    ),
                    sanitize_terminal(&contributor.title).into(),
                    String::new().into(),
                    // The list keeps the cell short; the full wording stays in the label
                    // and the contributor's details.
                    match (contributor.health, contributor.measured_seconds) {
                        (None, None)
                            if matches!(
                                card.kind,
                                MetricKind::OpenPrs | MetricKind::NeedsMyReview
                            ) =>
                        {
                            crate::ui_table::Cell::styled("current", crate::theme::Role::TextMuted)
                        }
                        (None, None) => crate::ui_table::Cell::styled(
                            crate::ui_table::NONE,
                            crate::theme::Role::TextMuted,
                        ),
                        _ => crate::ui_table::Cell::styled(
                            sanitize_terminal(&measurement),
                            role.role(),
                        ),
                    },
                ];
                contributors.push(contributor_row);
                let rows = contributor_details(&snapshot, &observations, &canonical, contributor)?;
                prepared_budget(&rows, &mut total_rows, &mut prepared_bytes)?;
                details.push(Arc::from(rows));
            }
            let mut card_row = row(
                format!("{title}: {}", value_label(&card.value)),
                coverage_role(&card.coverage),
                Some(MetricsWorkspaceAction::Metric(MetricsAction::Card(
                    card.kind,
                ))),
            );
            card_row.cells = vec![title.clone().into(), value_label(&card.value).into()];
            card_rows.push(card_row);
            cards.push(MetricsPreparedCard {
                title,
                summary,
                coverage,
                formula_rows: Arc::from(formula_rows),
                contributors: Arc::from(contributors),
                details: Arc::from(details),
            });
            prepared_budget(
                &cards.last().unwrap().contributors,
                &mut total_rows,
                &mut prepared_bytes,
            )?;
        }
        let mut repository_rows = vec![row(
            format!("[All {} declared repositories]", declared.len()),
            SemanticToken::Reference,
            Some(MetricsWorkspaceAction::Metric(MetricsAction::AllSelected)),
        )];
        for repository in &declared {
            repository_rows.push(row(
                format!("[{repository}]"),
                SemanticToken::Reference,
                Some(MetricsWorkspaceAction::Metric(MetricsAction::Repository(
                    repository.clone(),
                ))),
            ));
        }
        prepared_budget(&card_rows, &mut total_rows, &mut prepared_bytes)?;
        prepared_budget(&repository_rows, &mut total_rows, &mut prepared_bytes)?;
        let scope_label = if snapshot.scope.repositories() == &declared {
            format!("Repositories: ALL {} declared", declared.len())
        } else {
            format!(
                "Repository: {}",
                snapshot.scope.repositories().first().unwrap()
            )
        };
        let cohort_label = match evidence.cohort {
            crate::metrics::PrCohortKind::SelectedRepositoryInventory => {
                "Cohort: selected-repository PR inventory (all states)"
            }
            crate::metrics::PrCohortKind::PersonalQueueUnion => {
                "Cohort: personal queue union (NOT repository-wide)"
            }
        }
        .into();
        let window_label = format!(
            "UTC window [{}, {})",
            utc_label(snapshot.window.start).trim_end_matches(" UTC"),
            utc_label(snapshot.window.end).trim_end_matches(" UTC")
        );
        let as_of_label = format!(
            "As-of {} • snapshots are not trends",
            utc_label(snapshot.as_of)
        );
        let compact = |coverage: &EvidenceCoverage| match coverage.state {
            CoverageState::Complete => "Complete",
            CoverageState::Partial(_) => "Partial",
            CoverageState::Unavailable(_) => "Unavailable",
        };
        let source_coverage_label = format!(
            "Sources: PR {} • Checks {} • Actions {}{}",
            compact(&evidence.pr_coverage),
            compact(&evidence.check_coverage),
            compact(&evidence.run_coverage),
            if missing_repositories.is_empty() {
                String::new()
            } else {
                format!(" • Missing {}", missing_repositories.len())
            }
        );
        let all_card_rows: Arc<[MetricsPreparedRow]> = Arc::from(card_rows);
        let mut prepared = Self {
            layout: Default::default(),
            restored_scope: None,
            all_card_rows: all_card_rows.clone(),
            layout_rows: Arc::from([]),
            layout_return: (MetricsView::Cards, 0, 0),
            evidence,
            snapshot,
            observations,
            repositories,
            missing_repositories: Arc::from(missing_repositories),
            read_guard: None,
            declared_repositories: Arc::from(declared.into_iter().collect::<Vec<_>>()),
            scope_label,
            cohort_label,
            window_label,
            as_of_label,
            source_coverage_label,
            card_rows: all_card_rows,
            contributor_positions: vec![(0, 0); cards.len()],
            cards: Arc::from(cards),
            repository_rows: Arc::from(repository_rows),
            view: MetricsView::Cards,
            selected: 0,
            top: 0,
            pending: None,
            status: None,
            card_position: (0, 0),
            repository_return: (MetricsView::Cards, 0, 0),
            info_return: (MetricsView::Cards, 0, 0),
            canonical_index: Arc::new(canonical),
            trends: Arc::from([]),
            trend_return: (MetricsView::Cards, 0, 0),
            trend_day_return: (0, 0),
            stale_rows: Arc::from([]),
            stale_summary: String::new(),
            stale_return: (MetricsView::Cards, 0, 0),
            thread_rows: Arc::from([]),
            thread_summary: None,
            threads_return: (MetricsView::Cards, 0, 0),
            stale_days: STALE_DAYS,
            active_dashboard: None,
            saved_dashboards: Arc::from([]),
            saved_dashboards_note: None,
            dashboard_rows: Arc::from([]),
            dashboards_return: (MetricsView::Cards, 0, 0),
            name_entry: None,
            delete_armed: None,
        };
        prepared.prepare_stale();
        prepared.rebuild_layout()?;
        prepared.rebuild_dashboard_rows();
        Ok(prepared)
    }
    pub fn rows(&self) -> &[MetricsPreparedRow] {
        if self.pending.is_some() {
            return &[];
        }
        match self.view {
            MetricsView::Layout => &self.layout_rows,
            MetricsView::Cards => &self.card_rows,
            MetricsView::Contributors(card) => &self.cards[card].contributors,
            MetricsView::Details { card, contributor } => &self.cards[card].details[contributor],
            MetricsView::CardInfo(card) => &self.cards[card].formula_rows,
            MetricsView::Repositories => &self.repository_rows,
            MetricsView::Trend(card) => &self.trends[card].rows,
            MetricsView::TrendDay { card, day } => &self.trends[card].days[day],
            MetricsView::Stale => &self.stale_rows,
            MetricsView::Threads => &self.thread_rows,
            MetricsView::Dashboards => &self.dashboard_rows,
        }
    }
    /// The exact canonical PR target of an original observation.
    pub fn pr_target(observation: &MetricsPrObservation) -> MetricTarget {
        MetricTarget::PullRequest(crate::metrics::PrTarget {
            repository: observation.repository.full_name.to_ascii_lowercase(),
            number: observation.number,
            repository_id: Some(observation.repository.id),
            database_id: Some(observation.id),
            node_id: Some(observation.node_id.clone()),
            source_record_id: None,
        })
    }
    /// Open of one original observation, or why it cannot open.
    pub fn open_pr_action(&self, observation: &MetricsPrObservation) -> MetricsWorkspaceAction {
        open_action(
            &self.snapshot,
            &self.observations,
            &self.canonical_index,
            &Self::pr_target(observation),
        )
    }
    fn prepare_stale(&mut self) {
        let as_of = self.snapshot.as_of;
        let days = self.stale_days;
        let cutoff = as_of.saturating_sub(days.saturating_mul(86_400));
        let open = self
            .observations
            .iter()
            .filter(|pr| matches!(pr.state, crate::providers::gh::MetricsPrState::Open))
            .collect::<Vec<_>>();
        let unknown = open
            .iter()
            .filter(|pr| pr.observed_updated_at.is_none())
            .count();
        let mut stale = open
            .into_iter()
            .filter_map(|pr| {
                let updated = pr.observed_updated_at.as_ref()?.unix_seconds();
                (updated <= cutoff).then_some((updated, pr))
            })
            .collect::<Vec<_>>();
        stale.sort_by(|(a, left), (b, right)| {
            a.cmp(b)
                .then_with(|| left.repository.full_name.cmp(&right.repository.full_name))
                .then_with(|| left.number.cmp(&right.number))
        });
        let host = self.snapshot.scope.host().to_owned();
        self.stale_rows = stale
            .iter()
            .map(|(updated, pr)| {
                let item = format!("{}#{}", pr.repository.full_name, pr.number);
                let idle = format!("idle {}d", as_of.saturating_sub(*updated) / 86_400);
                let title = if pr.draft == Some(true) {
                    format!("Draft: {}", pr.title)
                } else {
                    pr.title.clone()
                };
                let mut stale_row = row(
                    sanitize_terminal(&format!(
                        "{} • {item} • {title} • {idle} • updated {}",
                        pr.author.actor.login,
                        utc_label(*updated)
                    )),
                    SemanticToken::Pending,
                    Some(self.open_pr_action(pr)),
                );
                stale_row.actor = Some(ActorIdentity::github(&host, &pr.author.actor));
                stale_row.avatar_only = true;
                stale_row.cells = vec![
                    crate::ui_table::Cell::styled(
                        sanitize_terminal(&item),
                        crate::theme::Role::TextMuted,
                    ),
                    sanitize_terminal(&title).into(),
                    String::new().into(),
                    crate::ui_table::Cell::styled(idle, SemanticToken::Pending.role()),
                ];
                stale_row
            })
            .collect();
        self.stale_summary = format!(
            "{} open PRs not updated for {days}+ days (GitHub updated_at at or before {}){}",
            self.stale_rows.len(),
            utc_label(cutoff),
            if unknown == 0 {
                String::new()
            } else {
                format!(" • {unknown} without updated_at (not counted)")
            }
        );
    }
    /// Worker-only: projects the on-demand unresolved-thread read and shows it.
    pub fn attach_threads(&mut self, rows: Vec<MetricsPreparedRow>, summary: String) {
        self.thread_rows = Arc::from(rows);
        self.thread_summary = Some(sanitize_terminal(&summary));
        // The Unresolved threads card shows what this read found.
        let _ = self.rebuild_layout();
        if !matches!(self.view, MetricsView::Threads) {
            self.remember();
            self.threads_return = (self.view, self.selected, self.top);
        }
        self.view = MetricsView::Threads;
        self.selected = 0;
        self.top = 0;
    }
    /// Worker-only: prepares every card's trend from retained points (or why they are
    /// unavailable). Drawing only reads these rows.
    pub fn attach_trends(
        &mut self,
        retained: Result<crate::metrics_trend_v1::RetainedTrends, String>,
    ) {
        self.trends = self
            .snapshot
            .cards
            .iter()
            .map(|card| crate::metrics_trend_v1::prepare(card.kind, &self.snapshot, &retained))
            .collect();
        if matches!(
            self.view,
            MetricsView::Trend(_) | MetricsView::TrendDay { .. }
        ) {
            (self.view, self.selected, self.top) = self.trend_return;
        }
    }
    pub fn selected_action(&self) -> Option<MetricsWorkspaceAction> {
        self.rows()
            .get(self.selected)
            .and_then(|row| row.action.clone())
    }
    pub fn window_action(&self, days: u64) -> MetricsWorkspaceAction {
        if !self.missing_repositories.is_empty() {
            return MetricsWorkspaceAction::Explain("Canonical headers are missing for part of the explicit source scope; Refresh before changing the cached window".into());
        }
        let as_of = self.snapshot.as_of;
        match UtcWindow::new(as_of.saturating_sub(days.saturating_mul(86_400)), as_of) {
            Ok(window) => MetricsWorkspaceAction::Metric(MetricsAction::Window(window)),
            Err(reason) => MetricsWorkspaceAction::Explain(reason),
        }
    }
    fn remember(&mut self) {
        match self.view {
            MetricsView::Cards => self.card_position = (self.selected, self.top),
            MetricsView::Contributors(card) => {
                self.contributor_positions[card] = (self.selected, self.top)
            }
            _ => {}
        }
    }
    pub fn apply_layout(
        &mut self,
        layout: crate::dashboard_profile_v1::DashboardLayoutV1,
    ) -> Result<(), String> {
        layout.validate()?;
        self.layout = layout;
        self.rebuild_layout()?;
        if self.view == MetricsView::Cards {
            self.selected = self.selected.min(self.card_rows.len().saturating_sub(1));
            self.top = self.top.min(self.selected);
        }
        Ok(())
    }
    fn displayed_card(&self, index: usize) -> Option<usize> {
        let MetricsWorkspaceAction::Metric(MetricsAction::Card(kind)) =
            self.card_rows.get(index)?.action.as_ref()?
        else {
            return None;
        };
        self.snapshot
            .cards
            .iter()
            .position(|card| card.kind == *kind)
    }
    fn rebuild_layout(&mut self) -> Result<(), String> {
        self.layout.validate()?;
        let cards = self
            .layout
            .shown()
            .map(|widget| match widget.metric() {
                Some(kind) => self
                    .all_card_rows
                    .iter()
                    .find(|row| {
                        row.action
                            == Some(MetricsWorkspaceAction::Metric(MetricsAction::Card(kind)))
                    })
                    .cloned()
                    .ok_or("Original dashboard widget is unavailable"),
                None => Ok(self.widget_card_row(widget)),
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.card_rows = Arc::from(cards);
        let last = self.card_rows.len().saturating_sub(1);
        self.card_position.0 = self.card_position.0.min(last);
        self.card_position.1 = self.card_position.1.min(self.card_position.0);
        for position in [
            &mut self.layout_return,
            &mut self.repository_return,
            &mut self.info_return,
            &mut self.trend_return,
            &mut self.stale_return,
            &mut self.threads_return,
            &mut self.dashboards_return,
        ] {
            if position.0 == MetricsView::Cards {
                position.1 = position.1.min(last);
                position.2 = position.2.min(position.1);
            }
        }
        let mut layout_rows = self
            .layout
            .slots()
            .iter()
            .enumerate()
            .map(|(index, slot)| {
                let label = slot.widget.label();
                row(
                    format!("[{}] {label}", if slot.visible { "x" } else { " " }),
                    if slot.visible {
                        SemanticToken::Success
                    } else {
                        SemanticToken::Muted
                    },
                    Some(MetricsWorkspaceAction::ToggleWidget(index)),
                )
            })
            .collect::<Vec<_>>();
        // The last Layout row cycles the stale threshold through STALE_THRESHOLDS.
        layout_rows.push(row(
            format!(
                "Stale threshold: {} days (Space or click cycles {})",
                self.stale_days,
                crate::dashboard_profile_v1::STALE_THRESHOLDS
                    .map(|days| days.to_string())
                    .join("/")
            ),
            SemanticToken::Reference,
            Some(MetricsWorkspaceAction::ToggleWidget(
                self.layout.slots().len(),
            )),
        ));
        self.layout_rows = Arc::from(layout_rows);
        Ok(())
    }
    /// The Cards row of an attention widget: its name, then what it currently holds.
    fn widget_card_row(
        &self,
        widget: crate::dashboard_profile_v1::DashboardWidget,
    ) -> MetricsPreparedRow {
        use crate::dashboard_profile_v1::DashboardWidget;
        let (value, role, action) = if widget == DashboardWidget::StalePrs {
            (
                format!("{} idle {}+ days", self.stale_rows.len(), self.stale_days),
                if self.stale_rows.is_empty() {
                    SemanticToken::Success
                } else {
                    SemanticToken::Pending
                },
                MetricsWorkspaceAction::Stale,
            )
        } else {
            (
                match &self.thread_summary {
                    Some(_) => format!("{} listed • Enter reads again", self.thread_rows.len()),
                    None => "Not read • Enter reads current threads".into(),
                },
                SemanticToken::Reference,
                MetricsWorkspaceAction::Metric(MetricsAction::Threads),
            )
        };
        let title = widget.label();
        let mut card_row = row(format!("{title}: {value}"), role, Some(action));
        card_row.cells = vec![title.into(), value.into()];
        card_row
    }
    /// The Dashboards list: each saved dashboard switches by Enter or click.
    fn rebuild_dashboard_rows(&mut self) {
        self.dashboard_rows = self
            .saved_dashboards
            .iter()
            .map(|entry| {
                row(
                    sanitize_terminal(&format!(
                        "{}{} • {} repositories • {} days • stale {}d",
                        entry.name,
                        if entry.active { " • active" } else { "" },
                        entry.repositories,
                        entry.window_days,
                        entry.stale_days
                    )),
                    if entry.active {
                        SemanticToken::Success
                    } else {
                        SemanticToken::Normal
                    },
                    Some(MetricsWorkspaceAction::Metric(
                        MetricsAction::SwitchDashboard(entry.name.clone()),
                    )),
                )
            })
            .collect();
        if self.view == MetricsView::Dashboards {
            self.selected = self
                .selected
                .min(self.dashboard_rows.len().saturating_sub(1));
            self.top = self.top.min(self.selected);
        }
    }
    /// Worker-only: the account's saved dashboards as just read, or why they could not be.
    pub fn attach_saved_dashboards(
        &mut self,
        saved: Result<Option<crate::dashboard_profile_v1::SavedDashboards>, String>,
    ) {
        match saved {
            Ok(saved) => {
                self.saved_dashboards = Arc::from(
                    saved
                        .as_ref()
                        .map(|saved| saved.entries())
                        .unwrap_or_default(),
                );
                self.saved_dashboards_note = None;
            }
            Err(error) => {
                self.saved_dashboards = Arc::from([]);
                self.saved_dashboards_note = Some(sanitize_terminal(&format!(
                    "Saved dashboards unreadable: {error}"
                )));
            }
        }
        if self.active_dashboard.as_ref().is_some_and(|active| {
            !self
                .saved_dashboards
                .iter()
                .any(|entry| &entry.name == active)
        }) {
            self.active_dashboard = None;
        }
        self.rebuild_dashboard_rows();
    }
    /// The saved dashboard selected in the Dashboards list.
    fn selected_saved(&self) -> Result<String, String> {
        if self.view != MetricsView::Dashboards {
            return Err("Open Dashboards (N) and select a saved dashboard first".into());
        }
        self.saved_dashboards
            .get(self.selected)
            .map(|entry| entry.name.clone())
            .ok_or_else(|| "No saved dashboard is selected; n New saves this one".into())
    }
    /// Applies a restored dashboard's stale threshold.
    pub fn apply_stale_days(&mut self, days: u64) -> Result<(), String> {
        if !crate::dashboard_profile_v1::STALE_THRESHOLDS.contains(&days) {
            return Err("Stale threshold is not one of the offered day counts".into());
        }
        self.stale_days = days;
        self.prepare_stale();
        self.rebuild_layout()
    }
    /// Returns only actual worker requests. Local Card/Back/picker/detail/selection is consumed.
    pub fn apply(
        &mut self,
        action: MetricsWorkspaceAction,
    ) -> Result<Option<MetricsAction>, String> {
        if let MetricsWorkspaceAction::Explain(reason) = action {
            return Err(reason);
        }
        if self.pending.is_some()
            && !matches!(action, MetricsWorkspaceAction::Metric(MetricsAction::Back))
        {
            return Err("Dashboard is preparing; old values and Open are hidden".into());
        }
        match action {
            MetricsWorkspaceAction::Layout => {
                self.remember();
                if self.view != MetricsView::Layout {
                    self.layout_return = (self.view, self.selected, self.top);
                    self.view = MetricsView::Layout;
                    self.selected = 0;
                    self.top = 0;
                }
            }
            MetricsWorkspaceAction::ToggleWidget(index) => {
                if self.view != MetricsView::Layout {
                    return Err("Open Layout before changing its widgets".into());
                }
                if index == self.layout.slots().len() {
                    // The stale threshold row cycles to the next offered day count.
                    let thresholds = crate::dashboard_profile_v1::STALE_THRESHOLDS;
                    let next = thresholds
                        .iter()
                        .position(|days| *days == self.stale_days)
                        .map_or(STALE_DAYS, |at| thresholds[(at + 1) % thresholds.len()]);
                    self.apply_stale_days(next)?;
                } else {
                    self.layout.toggle(index)?;
                    self.rebuild_layout()?;
                }
                self.selected = index;
            }
            MetricsWorkspaceAction::MoveWidget(down) => {
                if self.view != MetricsView::Layout {
                    return Err("Open Layout before reordering widgets".into());
                }
                if self.selected >= self.layout.slots().len() {
                    return Err(
                        "The stale threshold row stays last; select a widget to move".into(),
                    );
                }
                self.selected = self.layout.move_slot(self.selected, down)?;
                self.rebuild_layout()?;
            }
            MetricsWorkspaceAction::Select(index) => {
                self.selected = index.min(self.rows().len().saturating_sub(1));
                if self.delete_armed.is_some() {
                    self.delete_armed = None;
                    self.status = Some("Delete not confirmed • saved dashboards unchanged".into());
                }
            }
            MetricsWorkspaceAction::Dashboards => {
                if self.view == MetricsView::Dashboards {
                    return Ok(None);
                }
                if self.view == MetricsView::Layout {
                    return Err("Return to dashboard cards before switching dashboards".into());
                }
                self.remember();
                self.dashboards_return = (self.view, self.selected, self.top);
                self.view = MetricsView::Dashboards;
                self.selected = self
                    .saved_dashboards
                    .iter()
                    .position(|entry| entry.active)
                    .unwrap_or(0);
                self.top = 0;
                self.delete_armed = None;
            }
            MetricsWorkspaceAction::NewDashboard => {
                if self.view != MetricsView::Dashboards {
                    return Err("Open Dashboards (N) before naming a new dashboard".into());
                }
                self.delete_armed = None;
                self.name_entry = Some(NameEntry {
                    rename: None,
                    text: String::new(),
                });
            }
            MetricsWorkspaceAction::RenameDashboard => {
                let name = self.selected_saved()?;
                self.delete_armed = None;
                self.name_entry = Some(NameEntry {
                    rename: Some(name.clone()),
                    text: name,
                });
            }
            MetricsWorkspaceAction::DeleteDashboard => {
                let name = self.selected_saved()?;
                if self.delete_armed.as_deref() != Some(name.as_str()) {
                    self.status = Some(format!(
                        "Delete saved dashboard {name}? x again confirms • any other key keeps it"
                    ));
                    self.delete_armed = Some(name);
                    return Ok(None);
                }
                self.delete_armed = None;
                return self.apply(MetricsWorkspaceAction::Metric(
                    MetricsAction::DeleteDashboard(name),
                ));
            }
            MetricsWorkspaceAction::NameChar(character) => {
                let entry = self
                    .name_entry
                    .as_mut()
                    .ok_or("No dashboard name is being typed")?;
                if !character.is_control()
                    && entry.text.chars().count() < crate::dashboard_profile_v1::MAX_NAME_CHARS
                {
                    entry.text.push(character);
                }
            }
            MetricsWorkspaceAction::NameBackspace => {
                self.name_entry
                    .as_mut()
                    .ok_or("No dashboard name is being typed")?
                    .text
                    .pop();
            }
            MetricsWorkspaceAction::NameCancel => {
                self.name_entry = None;
                self.status = Some("Name discarded • saved dashboards unchanged".into());
            }
            MetricsWorkspaceAction::NameSubmit => {
                let entry = self
                    .name_entry
                    .clone()
                    .ok_or("No dashboard name is being typed")?;
                let name = crate::dashboard_profile_v1::validate_name(&entry.text)?;
                let request = match entry.rename {
                    Some(from) if from == name => {
                        self.name_entry = None;
                        return Err("The name is unchanged; nothing renamed".into());
                    }
                    Some(from) => MetricsAction::RenameDashboard { from, to: name },
                    None => MetricsAction::SaveDashboardAs(name),
                };
                self.name_entry = None;
                return self.apply(MetricsWorkspaceAction::Metric(request));
            }
            MetricsWorkspaceAction::Repositories => {
                self.remember();
                self.repository_return = (self.view, self.selected, self.top);
                self.view = MetricsView::Repositories;
                self.selected = 0;
                self.top = 0;
            }
            MetricsWorkspaceAction::Inspect(index) => {
                let original = (self.view, self.selected, self.top);
                match self.view {
                    MetricsView::Layout => {
                        return Err(
                            "Return to dashboard cards before inspecting their evidence".into()
                        )
                    }
                    MetricsView::Contributors(card) if index < self.cards[card].details.len() => {
                        self.selected = index;
                        self.remember();
                        self.view = MetricsView::Details {
                            card,
                            contributor: index,
                        };
                    }
                    MetricsView::Contributors(card) | MetricsView::CardInfo(card) => {
                        self.remember();
                        self.info_return = original;
                        self.view = MetricsView::CardInfo(card);
                    }
                    MetricsView::Cards => {
                        let selected = index.min(self.card_rows.len().saturating_sub(1));
                        let card = self.displayed_card(selected).ok_or(
                            "Stale PRs and Unresolved threads have no formula details; Enter opens them",
                        )?;
                        self.selected = selected;
                        self.remember();
                        self.info_return = (MetricsView::Cards, self.selected, self.top);
                        self.view = MetricsView::CardInfo(card);
                    }
                    MetricsView::Details { card, .. }
                    | MetricsView::Trend(card)
                    | MetricsView::TrendDay { card, .. } => {
                        self.info_return = original;
                        self.view = MetricsView::CardInfo(card);
                    }
                    MetricsView::Repositories | MetricsView::Dashboards => {
                        return Err("Choose a row or Back before inspecting metric details".into())
                    }
                    MetricsView::Stale | MetricsView::Threads => {
                        return Err(
                            "Enter opens the selected PR; Back returns to the dashboard cards"
                                .into(),
                        )
                    }
                }
                self.selected = 0;
                self.top = 0;
            }
            MetricsWorkspaceAction::Trend(index) => {
                let card = match self.view {
                    MetricsView::Cards => self
                        .displayed_card(index.min(self.card_rows.len().saturating_sub(1)))
                        .ok_or("Trends cover the metric cards; select one of them first")?,
                    MetricsView::Contributors(card)
                    | MetricsView::CardInfo(card)
                    | MetricsView::Details { card, .. } => card,
                    MetricsView::Trend(_) | MetricsView::TrendDay { .. } => return Ok(None),
                    MetricsView::Layout
                    | MetricsView::Repositories
                    | MetricsView::Stale
                    | MetricsView::Threads
                    | MetricsView::Dashboards => {
                        return Err("Return to dashboard cards before opening a trend".into())
                    }
                };
                if self.trends.len() != self.cards.len() {
                    return Err(
                        "Trend history is not prepared for this Dashboard; Refresh first".into(),
                    );
                }
                if self.view == MetricsView::Cards {
                    self.selected = index.min(self.card_rows.len().saturating_sub(1));
                }
                self.remember();
                self.trend_return = (self.view, self.selected, self.top);
                self.view = MetricsView::Trend(card);
                self.selected = 0;
                self.top = 0;
            }
            MetricsWorkspaceAction::Stale => {
                if self.view == MetricsView::Stale {
                    return Ok(None);
                }
                if self.view == MetricsView::Layout {
                    return Err("Return to dashboard cards before opening stale PRs".into());
                }
                self.remember();
                self.stale_return = (self.view, self.selected, self.top);
                self.view = MetricsView::Stale;
                self.selected = 0;
                self.top = 0;
            }
            MetricsWorkspaceAction::TrendDay(day) => {
                let MetricsView::Trend(card) = self.view else {
                    return Err("Open a metric trend before inspecting a retained day".into());
                };
                if day >= self.trends[card].days.len() {
                    return Err("Retained trend day unavailable".into());
                }
                if let Some(index) = self.trends[card]
                    .rows
                    .iter()
                    .position(|row| row.action == Some(MetricsWorkspaceAction::TrendDay(day)))
                {
                    self.selected = index;
                }
                self.trend_day_return = (self.selected, self.top);
                self.view = MetricsView::TrendDay { card, day };
                self.selected = 0;
                self.top = 0;
            }
            MetricsWorkspaceAction::Metric(MetricsAction::Card(kind)) => {
                let card = self
                    .snapshot
                    .cards
                    .iter()
                    .position(|card| card.kind == kind)
                    .ok_or("Metric card unavailable")?;
                if self.view == MetricsView::Cards {
                    self.selected = self
                        .card_rows
                        .iter()
                        .position(|row| {
                            row.action
                                == Some(MetricsWorkspaceAction::Metric(MetricsAction::Card(kind)))
                        })
                        .ok_or("Dashboard widget is hidden; show it in Layout first")?;
                }
                self.remember();
                self.view = MetricsView::Contributors(card);
                (self.selected, self.top) = self.contributor_positions[card];
            }
            MetricsWorkspaceAction::Metric(MetricsAction::Back) => {
                if self.pending.is_some() {
                    return Err("Dashboard request is still preparing; cancel through the application before navigating back".into());
                }
                self.remember();
                self.delete_armed = None;
                self.name_entry = None;
                match self.view {
                    MetricsView::Dashboards => {
                        (self.view, self.selected, self.top) = self.dashboards_return;
                        if self.view == MetricsView::Cards {
                            self.selected =
                                self.selected.min(self.card_rows.len().saturating_sub(1));
                            self.top = self.top.min(self.selected);
                        }
                    }
                    MetricsView::Layout => {
                        (self.view, self.selected, self.top) = self.layout_return;
                        if self.view == MetricsView::Cards {
                            self.selected =
                                self.selected.min(self.card_rows.len().saturating_sub(1));
                            self.top = self.top.min(self.selected);
                        }
                    }
                    MetricsView::Cards => {}
                    MetricsView::Contributors(_) => {
                        self.view = MetricsView::Cards;
                        (self.selected, self.top) = self.card_position;
                    }
                    MetricsView::Details { card, .. } => {
                        self.view = MetricsView::Contributors(card);
                        (self.selected, self.top) = self.contributor_positions[card];
                    }
                    MetricsView::CardInfo(_) => {
                        (self.view, self.selected, self.top) = self.info_return;
                    }
                    MetricsView::Repositories => {
                        (self.view, self.selected, self.top) = self.repository_return;
                    }
                    MetricsView::Trend(_) => {
                        (self.view, self.selected, self.top) = self.trend_return;
                        if self.view == MetricsView::Cards {
                            self.selected =
                                self.selected.min(self.card_rows.len().saturating_sub(1));
                            self.top = self.top.min(self.selected);
                        }
                    }
                    MetricsView::TrendDay { card, .. } => {
                        self.view = MetricsView::Trend(card);
                        (self.selected, self.top) = self.trend_day_return;
                    }
                    MetricsView::Stale | MetricsView::Threads => {
                        (self.view, self.selected, self.top) = if self.view == MetricsView::Stale {
                            self.stale_return
                        } else {
                            self.threads_return
                        };
                        if self.view == MetricsView::Cards {
                            self.selected =
                                self.selected.min(self.card_rows.len().saturating_sub(1));
                            self.top = self.top.min(self.selected);
                        }
                    }
                }
            }
            MetricsWorkspaceAction::Metric(request) => {
                match &request {
                    MetricsAction::Open(target) => {
                        canonical_open(
                            &self.snapshot,
                            &self.observations,
                            &self.canonical_index,
                            target,
                        )?;
                        if let Some(index) = self.rows().iter().position(|row| {
                            row.action.as_ref()
                                == Some(&MetricsWorkspaceAction::Metric(request.clone()))
                        }) {
                            self.selected = index;
                        }
                        self.remember();
                        return Ok(Some(request));
                    }
                    MetricsAction::Repository(repository) => {
                        validate_repository(repository).map_err(|error| error.to_string())?;
                        if !self
                            .declared_repositories
                            .iter()
                            .any(|name| name.eq_ignore_ascii_case(repository))
                        {
                            return Err(
                                "Dashboard repository is outside the explicit declared set".into(),
                            );
                        }
                    }
                    MetricsAction::Window(window) => {
                        if !self.missing_repositories.is_empty() {
                            return Err("Canonical headers are missing; Refresh before cached window recomputation".into());
                        }
                        if window.start >= window.end || window.end > self.snapshot.as_of {
                            return Err("Dashboard UTC request must retain an immutable bounded as-of window".into());
                        }
                    }
                    MetricsAction::Threads => {
                        if self.view == MetricsView::Layout {
                            return Err(
                                "Return to dashboard cards before reading unresolved threads"
                                    .into(),
                            );
                        }
                    }
                    MetricsAction::AllSelected
                    | MetricsAction::Refresh
                    | MetricsAction::SaveDashboard
                    | MetricsAction::RestoreDashboard
                    | MetricsAction::DefaultDashboard => {}
                    MetricsAction::SaveDashboardAs(name)
                    | MetricsAction::SwitchDashboard(name)
                    | MetricsAction::DeleteDashboard(name)
                    | MetricsAction::RenameDashboard { to: name, .. } => {
                        crate::dashboard_profile_v1::validate_name(name)?;
                    }
                    _ => return Err("Unexpected Dashboard action".into()),
                }
                self.remember();
                self.pending = Some(request.clone());
                self.status=Some("Preparing requested Dashboard • prior values hidden • no automatic scope expansion".into());
                return Ok(Some(request));
            }
            MetricsWorkspaceAction::Explain(_) => unreachable!(),
        }
        Ok(None)
    }
}

#[cfg(test)]
#[path = "metrics_workspace_tests.rs"]
mod tests;
