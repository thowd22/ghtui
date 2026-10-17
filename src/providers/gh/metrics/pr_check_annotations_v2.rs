//! Undeclared bounded original-job annotations; no URL/ordinal write authority.
use super::*;

#[derive(Clone, Debug)]
pub struct PrCheckAnnotationLimitsV2 {
    pub max_pages: usize,
    pub max_rows: usize,
    pub max_row_bytes: usize,
    pub max_prepared_bytes: usize,
    pub transport: PrCheckRunReadLimits,
}
impl Default for PrCheckAnnotationLimitsV2 {
    fn default() -> Self {
        Self {
            max_pages: 10,
            max_rows: 1000,
            max_row_bytes: 128 * 1024,
            max_prepared_bytes: 2 * 1024 * 1024,
            transport: Default::default(),
        }
    }
}
impl PrCheckAnnotationLimitsV2 {
    fn selected(&self) -> GithubResult<MetricsReadLimits> {
        if !(1..=10).contains(&self.max_pages)
            || !(1..=1000).contains(&self.max_rows)
            || self.max_rows < 100
            || !(1..=128 * 1024).contains(&self.max_row_bytes)
            || !(1..=2 * 1024 * 1024).contains(&self.max_prepared_bytes)
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Annotations need positive bounded pages/rows/raw/prepared fields",
            ));
        }
        self.transport.selected()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrCheckAnnotationPhaseV2 {
    BeforeAnnotations,
    BeforeFinalProof,
}
#[derive(Clone, Debug, Default)]
pub struct PrCheckAnnotationCoverageV2 {
    pub requested_pages: usize,
    pub fetched_pages: usize,
    pub raw_rows: usize,
    pub admitted_rows: usize,
    pub excluded_rows: usize,
    pub api_exhausted: bool,
    pub advertised_count: Option<u64>,
    pub complete: bool,
    pub failure: Option<GithubErrorKind>,
    pub reasons: Vec<String>,
}
pub struct PrCheckAnnotationRowV2 {
    path: Option<String>,
    start: Option<u64>,
    end: Option<u64>,
    start_column: Option<u64>,
    end_column: Option<u64>,
    level: Option<String>,
    title: Option<String>,
    message: Option<String>,
    details: Option<String>,
    display: Vec<String>,
}
impl PrCheckAnnotationRowV2 {
    pub fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }
    pub fn lines(&self) -> Option<(u64, u64)> {
        self.start.zip(self.end).filter(|(a, b)| *a > 0 && b >= a)
    }
    pub fn columns(&self) -> Option<(u64, u64)> {
        self.start_column
            .zip(self.end_column)
            .filter(|(a, b)| *a > 0 && b >= a)
    }
    pub fn level(&self) -> Option<&str> {
        self.level.as_deref()
    }
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
    pub fn details(&self) -> Option<&str> {
        self.details.as_deref()
    }
    pub fn prepared_display(&self) -> &[String] {
        &self.display
    }
}
pub struct PrCheckAnnotationsReadV2 {
    origin: Arc<PrCheckRunObservation>,
    identity: HostIdentity,
    target: Option<PrCheckCapturedTargetV2>,
    check_id: Option<u64>,
    check_node: Option<String>,
    rows: Vec<PrCheckAnnotationRowV2>,
    coverage: PrCheckAnnotationCoverageV2,
    requests: usize,
    charged_bytes: usize,
    observed_through: u64,
}
impl fmt::Debug for PrCheckAnnotationsReadV2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PrCheckAnnotationsReadV2(<closed bounded API-visible metadata>)")
    }
}
impl PrCheckAnnotationsReadV2 {
    pub(crate) fn workflow_owned_bytes_v9(&self) -> Option<usize> {
        use crate::providers::gh::workflow_capacity_v9 as b;
        let mut n = b::sum([
            b::owner::<Self>(),
            self.origin.workflow_owned_bytes_v9()?,
            b::identity(&self.identity)?,
            self.check_node.as_ref().map_or(0, String::capacity),
            match &self.target {
                Some(target) => target.workflow_owned_bytes_v9()?,
                None => 0,
            },
            b::vector(&self.rows)?,
            b::lines(&self.coverage.reasons)?,
        ])?;
        for row in &self.rows {
            n = n
                .checked_add(b::strings(
                    [
                        row.path.as_ref(),
                        row.level.as_ref(),
                        row.title.as_ref(),
                        row.message.as_ref(),
                        row.details.as_ref(),
                    ]
                    .into_iter()
                    .flatten(),
                )?)?
                .checked_add(b::lines(&row.display)?)?;
        }
        Some(n)
    }
    pub fn origin(&self) -> &Arc<PrCheckRunObservation> {
        &self.origin
    }
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn target(&self) -> Option<&PrCheckCapturedTargetV2> {
        self.target.as_ref()
    }
    pub fn check_id(&self) -> Option<u64> {
        self.check_id
    }
    pub fn check_node(&self) -> Option<&str> {
        self.check_node.as_deref()
    }
    pub fn rows(&self) -> &[PrCheckAnnotationRowV2] {
        &self.rows
    }
    pub fn coverage(&self) -> &PrCheckAnnotationCoverageV2 {
        &self.coverage
    }
    pub fn requests(&self) -> usize {
        self.requests
    }
    pub fn charged_bytes(&self) -> usize {
        self.charged_bytes
    }
    pub fn observed_through(&self) -> u64 {
        self.observed_through
    }
}
fn check_selector(provider: &GithubProvider, repo: &str, value: &str) -> GithubResult<u64> {
    // Selector only: actual fresh CheckRun below is mandatory before SQL/body.
    let id = value
        .rsplit('/')
        .next()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|n| *n > 0)
        .filter(|id| {
            exact_api_url(
                &provider.host,
                &format!("repos/{repo}/check-runs/{id}"),
                &Value::String(value.to_owned()),
            )
        })
        .ok_or_else(|| {
            error(
                GithubErrorKind::Unsupported,
                "Actual member has no canonical check selector; no URL/default probe",
            )
        })?;
    Ok(id)
}
fn annotation_check(
    provider: &GithubProvider,
    original: &PrCheckOriginal,
    id: u64,
    raw: &Value,
) -> GithubResult<CheckProof> {
    if positive_bounded(raw, "id")? != id
        || raw["head_sha"].as_str() != Some(original.head.as_str())
        || !exact_api_url(
            &provider.host,
            &format!("repos/{}/check-runs/{id}", original.repository),
            &raw["url"],
        )
    {
        return Err(error(
            GithubErrorKind::IdentityChanged,
            "Job-linked check ID/head/origin changed",
        ));
    }
    Ok(CheckProof {
        node: node(raw, "node_id")?,
        suite: positive_bounded(&raw["check_suite"], "id")?,
    })
}
fn row(raw: &Value, coverage: &mut PrCheckAnnotationCoverageV2) -> PrCheckAnnotationRowV2 {
    let field = |key: &str, cap: usize| {
        raw.get(key)
            .and_then(Value::as_str)
            .filter(|s| s.len() <= cap)
            .map(str::to_owned)
    };
    let start = raw["start_line"].as_u64().filter(|n| *n > 0);
    let end = raw["end_line"].as_u64().filter(|n| *n > 0);
    let lines = start.zip(end).filter(|(a, b)| b >= a);
    let mut start_column = raw["start_column"].as_u64().filter(|n| *n > 0);
    let mut end_column = raw["end_column"].as_u64().filter(|n| *n > 0);
    if lines.is_none_or(|(a, b)| a != b) || start_column.zip(end_column).is_some_and(|(a, b)| b < a)
    {
        start_column = None;
        end_column = None;
    }
    let mut item = PrCheckAnnotationRowV2 {
        path: field("path", 4096).filter(|s| !s.is_empty() && !s.contains('\0')),
        start,
        end,
        start_column,
        end_column,
        level: field("annotation_level", 32)
            .filter(|s| matches!(s.as_str(), "notice" | "warning" | "failure")),
        title: field("title", 1020).filter(|s| s.chars().count() <= 255),
        message: field("message", 64 * 1024),
        details: field("raw_details", 64 * 1024),
        display: vec![],
    };
    // Prepare bounded sanitized display once in worker, never split/regex/body
    // copying in draw. Raw reported candidate fields remain separate.
    for text in [
        item.title.as_deref().unwrap_or("Title unavailable"),
        item.message.as_deref().unwrap_or("Message unavailable"),
        item.details.as_deref().unwrap_or("Raw details unavailable"),
    ] {
        for line in text.lines() {
            if item.display.len() >= 4096 {
                coverage.reasons.push(
                    "Annotation detail display line limit; reported raw fields retained".into(),
                );
                break;
            }
            item.display
                .push(crate::providers::runner::sanitize_terminal(line));
        }
    }
    if item.path.is_none()
        || item.lines().is_none()
        || item.level.is_none()
        || item.message.is_none()
    {
        coverage
            .reasons
            .push("Some annotation fields unavailable; no line0/canonical range inferred".into());
    }
    item
}
fn read_pages(
    provider: &GithubProvider,
    budget: &mut MetricsBudget,
    path: &str,
    count: Option<u64>,
    limits: &PrCheckAnnotationLimitsV2,
    cancel: &CancellationToken,
) -> GithubResult<(Vec<PrCheckAnnotationRowV2>, PrCheckAnnotationCoverageV2)> {
    let mut coverage = PrCheckAnnotationCoverageV2 {
        advertised_count: count,
        ..Default::default()
    };
    let mut rows = vec![];
    let mut prepared = 0usize;
    for page in 1..=limits.max_pages {
        if coverage
            .raw_rows
            .checked_add(100)
            .is_none_or(|n| n > limits.max_rows)
        {
            coverage
                .reasons
                .push("Fixed annotation row cap reached before request".into());
            break;
        }
        coverage.requested_pages += 1;
        let raw = match budget.read_document(
            provider,
            &format!("{path}/annotations?per_page=100&page={page}"),
            cancel,
        ) {
            Ok(v) => v,
            Err(e) => {
                if matches!(
                    e.kind,
                    GithubErrorKind::Authentication | GithubErrorKind::IdentityChanged
                ) {
                    return Err(e);
                }
                coverage.failure = Some(e.kind);
                coverage.reasons.push(
                    "Annotations endpoint unavailable; known facts are not complete/empty".into(),
                );
                break;
            }
        };
        let values = raw
            .as_array()
            .filter(|rows| rows.len() <= 100)
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Json,
                    "Annotation endpoint omitted its bounded array",
                )
            })?;
        coverage.fetched_pages += 1;
        coverage.raw_rows += values.len();
        for value in values {
            let bytes = serde_json::to_vec(value)
                .map_err(|_| error(GithubErrorKind::Json, "Annotation row encoding unavailable"))?
                .len();
            if !value.is_object()
                || bytes > limits.max_row_bytes
                || prepared
                    .checked_add(bytes + std::mem::size_of::<PrCheckAnnotationRowV2>())
                    .is_none_or(|n| n > limits.max_prepared_bytes)
            {
                coverage.excluded_rows += 1;
                continue;
            }
            let item = row(value, &mut coverage);
            let display_bytes = item
                .display
                .iter()
                .try_fold(0usize, |n, s| {
                    n.checked_add(s.len())
                        .and_then(|n| n.checked_add(std::mem::size_of::<String>()))
                })
                .ok_or_else(|| {
                    error(
                        GithubErrorKind::Unsupported,
                        "Annotation display accounting overflow",
                    )
                })?;
            let retained = bytes
                .checked_add(display_bytes)
                .and_then(|n| n.checked_add(std::mem::size_of::<PrCheckAnnotationRowV2>()))
                .ok_or_else(|| {
                    error(
                        GithubErrorKind::Unsupported,
                        "Annotation row accounting overflow",
                    )
                })?;
            if prepared
                .checked_add(retained)
                .is_none_or(|n| n > limits.max_prepared_bytes)
            {
                coverage.excluded_rows += 1;
                continue;
            }
            prepared += retained;
            rows.push(item);
            coverage.admitted_rows += 1;
        }
        if values.len() < 100 {
            coverage.api_exhausted = true;
            break;
        }
    }
    coverage.complete = coverage.api_exhausted
        && coverage.excluded_rows == 0
        && coverage.failure.is_none()
        && coverage.reasons.is_empty()
        && count == Some(coverage.raw_rows as u64);
    if count.is_none() {
        coverage.reasons.push(
            "Advertised annotation total unavailable; API exhaustion is not a total proof".into(),
        );
    }
    if !coverage.complete {
        coverage
            .reasons
            .push("Bounded/partial API-visible annotations; no absence/history claim".into());
    }
    Ok((rows, coverage))
}
#[path = "pr_check_annotations_read_v2.rs"]
mod read;
