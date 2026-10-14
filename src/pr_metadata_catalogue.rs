//! Worker adapter for bounded, original-bound eligible metadata reads.
//! The retained provider observation is a snapshot, never durable authentication
//! or mutation authority. Local indexed selection performs no I/O. Publication
//! must load fresh evidence and revalidate the exact chosen value before journal
//! creation and the existing publication guards; no absent member is removed.
use crate::{
    pr_metadata_choices::{
        PrMetadataCatalogue, PrMetadataChoices, PrMetadataCoverage, PrMetadataLimits,
        PrMetadataOriginal, PrMetadataSelection,
    },
    pr_write_journal::PrWriteTarget,
    providers::{
        gh::{
            GithubError, GithubErrorKind, GithubProvider, GithubResult, HostIdentity,
            MetadataChoices, MetricsReadCoverage, MetricsReadStatus, MetricsRepository, PrContext,
            PrMetadataCatalogueRead, PrMetadataReadAccounting, PrMetadataReadLimits, PrSummary,
        },
        runner::CancellationToken,
    },
    search::model::valid_sha,
};
use std::{sync::Arc, time::Instant};

/// Keep this in an Arc when sharing with the native picker. No raw catalogue or
/// original description is cloned on local navigation, selection or rendering.
pub struct LoadedPrMetadataChoices {
    source: Arc<PrMetadataCatalogueRead>,
    prepared: PrMetadataChoices,
    read_limits: PrMetadataReadLimits,
    prepare_limits: PrMetadataLimits,
}
impl std::fmt::Debug for LoadedPrMetadataChoices {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("LoadedPrMetadataChoices(<bounded original-bound snapshot>)")
    }
}
impl LoadedPrMetadataChoices {
    /// Genuine read-produced snapshot, not reconstructed from prepared labels.
    pub fn source(&self) -> &Arc<PrMetadataCatalogueRead> {
        &self.source
    }
    pub fn prepared(&self) -> &PrMetadataChoices {
        &self.prepared
    }
    pub fn identity(&self) -> &HostIdentity {
        self.source.identity()
    }
    pub fn target(&self) -> &PrSummary {
        self.source.target()
    }
    pub fn context(&self) -> &PrContext {
        self.source.context()
    }
    /// Preserve source traversal, nullable metadata and exclusions independently;
    /// metric_evidence_complete is not a grant or global Base-picker denial.
    pub fn coverage(&self) -> &MetricsReadCoverage {
        self.source.coverage()
    }
    pub fn accounting(&self) -> &PrMetadataReadAccounting {
        self.source.accounting()
    }
    pub fn limits(&self) -> &PrMetadataReadLimits {
        &self.read_limits
    }
    pub fn preparation_limits(&self) -> PrMetadataLimits {
        self.prepare_limits
    }
    pub fn canonical_repository(&self) -> &MetricsRepository {
        self.source.canonical_repository()
    }
    pub fn observed_interval(&self) -> (u64, u64) {
        self.source.observed_interval()
    }
    pub fn choose(&self, index: usize) -> GithubResult<PrMetadataSelection> {
        self.prepared
            .choose(index)
            .map_err(|message| failure(GithubErrorKind::InvalidTarget, message))
    }
}

/// Own no implicit context/auth calls: the provider's sealed producer executes
/// exactly N+7 bounded API GETs, including its positive numeric-principal and
/// canonical repository/PR/issue checks before and after catalogue pages.
pub fn load(
    provider: &GithubProvider,
    original: PrMetadataOriginal<'_>,
    kind: MetadataChoices,
    read_limits: &PrMetadataReadLimits,
    prepare_limits: PrMetadataLimits,
    cancel: &CancellationToken,
) -> GithubResult<LoadedPrMetadataChoices> {
    load_selected(
        provider,
        original,
        kind,
        None,
        read_limits,
        prepare_limits,
        cancel,
    )
}
/// One genuine exact branch observation, explicitly NOT catalogue enumeration.
pub fn load_branch(
    provider: &GithubProvider,
    original: PrMetadataOriginal<'_>,
    branch: &str,
    read_limits: &PrMetadataReadLimits,
    prepare_limits: PrMetadataLimits,
    cancel: &CancellationToken,
) -> GithubResult<LoadedPrMetadataChoices> {
    load_selected(
        provider,
        original,
        MetadataChoices::Branches,
        Some(branch),
        read_limits,
        prepare_limits,
        cancel,
    )
}
fn load_selected(
    provider: &GithubProvider,
    original: PrMetadataOriginal<'_>,
    kind: MetadataChoices,
    branch: Option<&str>,
    read_limits: &PrMetadataReadLimits,
    prepare_limits: PrMetadataLimits,
    cancel: &CancellationToken,
) -> GithubResult<LoadedPrMetadataChoices> {
    let started = Instant::now();
    prepare_limits
        .validate()
        .map_err(|message| failure(GithubErrorKind::InvalidTarget, message))?;
    ready(started, read_limits, cancel)?;
    let original_key = canonical_original(provider, original, prepare_limits.max_field_bytes)?;
    let source = if let Some(branch) = branch {
        provider.metadata_branch_read(
            original.identity,
            original.target,
            original.context,
            branch,
            read_limits,
            cancel,
        )?
    } else {
        provider.metadata_catalogue_read(
            original.identity,
            original.target,
            original.context,
            kind,
            read_limits,
            cancel,
        )?
    };
    ready(started, read_limits, cancel)?;
    validate_read(
        provider,
        original,
        &original_key,
        kind,
        read_limits,
        prepare_limits.max_field_bytes,
        &source,
    )?;
    if source.repository_raw()["archived"].as_bool().is_none() {
        return Err(failure(GithubErrorKind::InvalidTarget,
            "Current repository archive state is unverified; no metadata mutation availability granted"));
    }
    let coverage = prepare_coverage(&source, prepare_limits.max_field_bytes);
    let prepared = PrMetadataChoices::prepare(
        PrMetadataOriginal {
            identity: source.identity(),
            target: source.target(),
            context: source.context(),
        },
        kind,
        PrMetadataCatalogue {
            rows: source.rows(),
            coverage,
        },
        prepare_limits,
    )
    .map_err(|message| failure(GithubErrorKind::InvalidTarget, message))?;
    ready(started, read_limits, cancel)?;
    Ok(LoadedPrMetadataChoices {
        source: Arc::new(source),
        prepared,
        read_limits: read_limits.clone(),
        prepare_limits,
    })
}

fn canonical_original(
    provider: &GithubProvider,
    original: PrMetadataOriginal<'_>,
    max_field_bytes: usize,
) -> GithubResult<PrWriteTarget> {
    // Only these immutable binding fields are copied/case-normalized here; the
    // unrelated description and raw source remain borrowed/retained unchanged.
    for value in [
        &original.identity.host,
        &original.identity.login,
        &original.target.pr.id,
        &original.target.pr.host,
        &original.target.pr.repository,
        &original.target.head_sha,
        &original.target.base_sha,
        &original.target.head_ref,
        &original.target.base_ref,
        &original.context.repository.host,
        &original.context.repository.full_name,
        &original.context.base_repository.host,
        &original.context.base_repository.full_name,
        &original.context.head_sha,
        &original.context.base_sha,
    ] {
        if value.len() > max_field_bytes {
            return Err(failure(
                GithubErrorKind::InvalidTarget,
                "Original metadata binding UTF-8 field budget exceeded before cloning",
            ));
        }
    }
    let target = PrWriteTarget::new(original.identity, original.target)
        .map_err(|error| failure(GithubErrorKind::InvalidTarget, error.to_string()))?;
    let target = PrWriteTarget::for_source(
        target.scope(),
        &target.repository().to_ascii_lowercase(),
        target.node_id(),
        target.number(),
    )
    .map_err(|error| failure(GithubErrorKind::InvalidTarget, error.to_string()))?;
    if original.identity.host != provider.host()
        || !valid_sha(&original.target.head_sha)
        || !valid_sha(&original.target.base_sha)
        || original.context.repository.id == 0
        || original.context.repository.id != original.context.base_repository.id
        || !original
            .context
            .repository
            .host
            .eq_ignore_ascii_case(provider.host())
        || !original
            .context
            .repository
            .full_name
            .eq_ignore_ascii_case(target.repository())
        || !original
            .context
            .base_repository
            .host
            .eq_ignore_ascii_case(provider.host())
        || !original
            .context
            .base_repository
            .full_name
            .eq_ignore_ascii_case(target.repository())
        || original.context.head_sha != original.target.head_sha
        || original.context.base_sha != original.target.base_sha
    {
        return Err(failure(
            GithubErrorKind::InvalidTarget,
            "Metadata loader requires an original positive canonical account/repository/PR/full head/base",
        ));
    }
    Ok(target)
}

fn validate_read(
    provider: &GithubProvider,
    original: PrMetadataOriginal<'_>,
    original_key: &PrWriteTarget,
    kind: MetadataChoices,
    limits: &PrMetadataReadLimits,
    max_field_bytes: usize,
    source: &PrMetadataCatalogueRead,
) -> GithubResult<()> {
    let current = PrMetadataOriginal {
        identity: source.identity(),
        target: source.target(),
        context: source.context(),
    };
    // Numeric Scope intentionally permits a legitimate same-UID login rename.
    if canonical_original(provider, current, max_field_bytes)? != *original_key
        || std::mem::discriminant(&source.kind()) != std::mem::discriminant(&kind)
        || source.canonical_repository().id != original.context.repository.id
        || source.canonical_repository().node_id.is_empty()
        || source.canonical_repository().host != provider.host()
        || !source
            .canonical_repository()
            .full_name
            .eq_ignore_ascii_case(original_key.repository())
        || source.target().head_sha != original.target.head_sha
        || source.target().base_sha != original.target.base_sha
        || source.target().head_ref != original.target.head_ref
        || source.target().base_ref != original.target.base_ref
    {
        return Err(failure(
            GithubErrorKind::IdentityChanged,
            "Read metadata snapshot differs from the original account/repository/PR/head/base/ref/kind",
        ));
    }
    let coverage = source.coverage();
    let accounted_rows = coverage
        .admitted_rows
        .saturating_add(coverage.excluded_rows);
    let (observed_from, observed_through) = source.observed_interval();
    if source.requests() != coverage.requested_pages.saturating_add(7)
        || coverage.requested_pages > limits.max_pages
        || coverage.fetched_pages > coverage.requested_pages
        || coverage.fetched_rows > limits.max_rows
        || coverage.admitted_rows != source.rows().len()
        || accounted_rows > coverage.fetched_rows
        || coverage.response_bytes > source.response_bytes()
        || source.accounting().charged_bytes() > limits.max_response_bytes
        || coverage.enumeration_complete && coverage.fetched_pages == 0
        || coverage.complete_empty
            != (coverage.enumeration_complete
                && coverage.fetched_rows == 0
                && coverage.excluded_rows == 0)
        || observed_through < observed_from
        || matches!(coverage.status, MetricsReadStatus::Unavailable(_)) && !source.rows().is_empty()
    {
        return Err(failure(
            GithubErrorKind::Json,
            "Bounded catalogue observation has inconsistent actual requests/rows/bytes/coverage",
        ));
    }
    Ok(())
}

fn prepare_coverage(
    source: &PrMetadataCatalogueRead,
    max_reason_bytes: usize,
) -> PrMetadataCoverage<'_> {
    let coverage = source.coverage();
    if let MetricsReadStatus::Unavailable(error) = &coverage.status {
        return PrMetadataCoverage::Unavailable {
            reason: bounded_reason(
                &error.message,
                max_reason_bytes,
                "Selected endpoint unavailable; inspect source diagnostics",
            ),
        };
    }
    // Traversal proof and candidate eligibility are separate from unrelated
    // nullable context diagnostics. Excluded rows never become complete absence.
    if coverage.enumeration_complete
        && coverage.excluded_rows == 0
        && coverage.requested_pages == coverage.fetched_pages
    {
        PrMetadataCoverage::Complete {
            fetched_pages: coverage.fetched_pages,
        }
    } else {
        PrMetadataCoverage::Partial {
            fetched_pages: coverage.fetched_pages,
            reason: coverage.reasons.first().map_or(
                "Bounded traversal/exclusions; inspect source diagnostics",
                |reason| {
                    bounded_reason(
                        reason,
                        max_reason_bytes,
                        "Bounded traversal/exclusions; inspect source diagnostics",
                    )
                },
            ),
        }
    }
}
fn bounded_reason<'a>(reason: &'a str, maximum: usize, fallback: &'static str) -> &'a str {
    if reason.len() <= maximum {
        reason
    } else {
        fallback
    }
}
fn ready(
    started: Instant,
    limits: &PrMetadataReadLimits,
    cancel: &CancellationToken,
) -> GithubResult<()> {
    if cancel.is_cancelled() || started.elapsed() >= limits.max_duration {
        Err(failure(
            GithubErrorKind::Transport,
            "Metadata catalogue cancelled or total read/preparation deadline reached; no late choices published",
        ))
    } else {
        Ok(())
    }
}
fn failure(kind: GithubErrorKind, message: impl Into<String>) -> GithubError {
    GithubError {
        kind,
        message: message.into(),
    }
}

#[cfg(test)]
#[path = "pr_metadata_catalogue_tests.rs"]
mod tests;
