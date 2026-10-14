use super::*;

#[derive(Clone, Debug)]
pub struct PrMetadataReadLimits {
    pub max_pages: usize,
    pub page_size: usize,
    pub max_rows: usize,
    /// Whole canonical/page JSON response cap, applied BEFORE every request.
    pub max_document_bytes: usize,
    pub max_row_bytes: usize,
    /// Combined kept stdout/stderr; failed calls reserve their shared cap.
    pub max_response_bytes: usize,
    pub max_duration: Duration,
}
impl Default for PrMetadataReadLimits {
    fn default() -> Self {
        Self {
            max_pages: 10,
            page_size: 100,
            max_rows: 1000,
            max_document_bytes: 128 * 1024,
            max_row_bytes: 16 * 1024,
            max_response_bytes: 1024 * 1024,
            max_duration: Duration::from_secs(30),
        }
    }
}
impl PrMetadataReadLimits {
    pub(super) fn as_metrics(&self) -> GithubResult<MetricsReadLimits> {
        if self.max_pages > 10
            || self.max_rows > 1000
            || !(1..=2 * 1024 * 1024).contains(&self.max_response_bytes)
            || !(1..=128 * 1024).contains(&self.max_document_bytes)
            || !(1..=64 * 1024).contains(&self.max_row_bytes)
            || self.max_duration > Duration::from_secs(60)
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Catalogue selected work budgets exceed bounded metadata limits",
            ));
        }
        let limits = MetricsReadLimits {
            max_pages: self.max_pages,
            page_size: self.page_size,
            max_rows: self.max_rows,
            max_document_bytes: self.max_document_bytes,
            max_response_bytes: self.max_response_bytes,
            max_duration: self.max_duration,
        };
        limits.validate()?;
        Ok(limits)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct PrMetadataReadAccounting {
    pub(super) requests: usize,
    pub(super) successful_stdout_bytes: usize,
    pub(super) successful_diagnostic_bytes: usize,
    pub(super) failed_reserved_bytes: usize,
}
impl PrMetadataReadAccounting {
    pub fn requests(&self) -> usize {
        self.requests
    }
    pub fn successful_stdout_bytes(&self) -> usize {
        self.successful_stdout_bytes
    }
    pub fn successful_diagnostic_bytes(&self) -> usize {
        self.successful_diagnostic_bytes
    }
    pub fn failed_reserved_bytes(&self) -> usize {
        self.failed_reserved_bytes
    }
    pub fn charged_bytes(&self) -> usize {
        self.successful_stdout_bytes
            .saturating_add(self.successful_diagnostic_bytes)
            .saturating_add(self.failed_reserved_bytes)
    }
}
/// No public constructor or serde; only the bounded transport emits this read.
pub struct PrMetadataCatalogueRead {
    pub(super) identity: HostIdentity,
    pub(super) target: PrSummary,
    pub(super) context: PrContext,
    pub(super) kind: MetadataChoices,
    pub(super) rows: Vec<Value>,
    pub(super) coverage: MetricsReadCoverage,
    pub(super) repository: MetricsRepository,
    pub(super) parent_raw: Value,
    pub(super) repository_raw: Value,
    pub(super) observed_interval: (u64, u64),
    pub(super) accounting: PrMetadataReadAccounting,
}
impl std::fmt::Debug for PrMetadataCatalogueRead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrMetadataCatalogueRead(<sealed bounded original context>)")
    }
}
impl PrMetadataCatalogueRead {
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn target(&self) -> &PrSummary {
        &self.target
    }
    pub fn original(&self) -> &PrSummary {
        &self.target
    }
    pub fn context(&self) -> &PrContext {
        &self.context
    }
    pub fn kind(&self) -> MetadataChoices {
        self.kind
    }
    pub fn rows(&self) -> &[Value] {
        &self.rows
    }
    pub fn coverage(&self) -> &MetricsReadCoverage {
        &self.coverage
    }
    pub fn canonical_repository(&self) -> &MetricsRepository {
        &self.repository
    }
    pub fn parent_raw(&self) -> &Value {
        &self.parent_raw
    }
    pub fn repository_raw(&self) -> &Value {
        &self.repository_raw
    }
    pub fn observed_interval(&self) -> (u64, u64) {
        self.observed_interval
    }
    pub fn accounting(&self) -> &PrMetadataReadAccounting {
        &self.accounting
    }
    pub fn requests(&self) -> usize {
        self.accounting.requests
    }
    pub fn requested_pages(&self) -> usize {
        self.coverage.requested_pages
    }
    pub fn fetched_pages(&self) -> usize {
        self.coverage.fetched_pages
    }
    pub fn raw_rows(&self) -> usize {
        self.coverage.fetched_rows
    }
    pub fn response_bytes(&self) -> usize {
        self.accounting.successful_stdout_bytes
    }
    pub fn successful_diagnostic_bytes(&self) -> usize {
        self.accounting.successful_diagnostic_bytes
    }
    pub fn failed_reserved_bytes(&self) -> usize {
        self.accounting.failed_reserved_bytes
    }
}
