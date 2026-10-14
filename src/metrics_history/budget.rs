use super::*;
use crate::providers::gh::{MetricsReadAccounting, MetricsReadLimits};
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct MetricsHistoryLimits {
    pub max_parents: usize,
    pub max_requests: usize,
    pub max_pages: usize,
    pub max_raw_rows: usize,
    pub max_charged_bytes: usize,
    pub max_duration: Duration,
    pub sql_lease: Duration,
    pub page_size: usize,
    pub source_pages: usize,
    pub source_rows: usize,
    pub source_bytes: usize,
    pub document_bytes: usize,
    pub retention: MetricsRetentionLimits,
}
impl Default for MetricsHistoryLimits {
    fn default() -> Self {
        Self {
            max_parents: 8,
            max_requests: 256,
            max_pages: 32,
            max_raw_rows: 4000,
            max_charged_bytes: 32 * 1024 * 1024,
            max_duration: Duration::from_secs(30),
            sql_lease: Duration::from_secs(5),
            page_size: 100,
            source_pages: 5,
            source_rows: 500,
            source_bytes: 8 * 1024 * 1024,
            document_bytes: 64 * 1024,
            retention: MetricsRetentionLimits {
                max_events: 512,
                max_document_bytes: 64 * 1024,
                max_total_bytes: 2 * 1024 * 1024,
                max_state_bytes: 128 * 1024,
            },
        }
    }
}
impl MetricsHistoryLimits {
    pub fn validate(&self) -> Result<()> {
        if !(1..=16).contains(&self.max_parents)
            || !(10..=1024).contains(&self.max_requests)
            || !(2..=200).contains(&self.max_pages)
            || !(2..=10000).contains(&self.max_raw_rows)
            || !(1..=64 * 1024 * 1024).contains(&self.max_charged_bytes)
            || self.max_duration.is_zero()
            || self.max_duration > Duration::from_secs(300)
            || self.sql_lease.is_zero()
            || self.sql_lease > Duration::from_secs(5)
            || !(1..=100).contains(&self.page_size)
            || !(1..=100).contains(&self.source_pages)
            || self.source_rows < self.page_size
            || self.source_rows > 10000
            || !(1..=32 * 1024 * 1024).contains(&self.source_bytes)
            || !(1..=256 * 1024).contains(&self.document_bytes)
            || !(1..=10000).contains(&self.retention.max_events)
            || !(1..=256 * 1024).contains(&self.retention.max_document_bytes)
            || !(1..=128 * 1024).contains(&self.retention.max_state_bytes)
            || self.retention.max_total_bytes > 2 * 1024 * 1024
            || self.retention.max_total_bytes == 0
        {
            bail!("History requires bounded positive global/source/retention caps and at most five-second SQL lease");
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MetricsHistoryUsage {
    pub requests: usize,
    pub requested_pages: usize,
    pub fetched_pages: usize,
    pub raw_rows: usize,
    pub successful_stdout_bytes: usize,
    pub successful_diagnostic_bytes: usize,
    pub failed_reserved_bytes: usize,
    pub charged_bytes: usize,
}
pub(super) struct HistoryBudget {
    limits: MetricsHistoryLimits,
    started: Instant,
    closing_requests: usize,
    usage: MetricsHistoryUsage,
}
impl HistoryBudget {
    pub fn new(limits: &MetricsHistoryLimits, parents: usize, repositories: usize) -> Result<Self> {
        limits.validate()?;
        if parents == 0 || parents > limits.max_parents || repositories == 0 || repositories > 64 {
            bail!("Explicit history parent/repository scope exceeds original hard budget");
        }
        let closing_requests = parents * 3 + repositories + 2;
        if 13 * parents + closing_requests > limits.max_requests {
            bail!(
                "Global request cap cannot fit initial proof/backfill and final whole-scope proof"
            );
        }
        Ok(Self {
            limits: limits.clone(),
            started: Instant::now(),
            closing_requests,
            usage: MetricsHistoryUsage::default(),
        })
    }
    pub fn check(&self, cancel: &CancellationToken) -> Result<()> {
        if cancel.is_cancelled() || self.started.elapsed() >= self.limits.max_duration {
            bail!("Original history deadline/cancellation reached; no source proof is renewed or resent");
        }
        if self.usage.requests > self.limits.max_requests
            || self.usage.requested_pages > self.limits.max_pages
            || self.usage.raw_rows > self.limits.max_raw_rows
            || self.usage.charged_bytes > self.limits.max_charged_bytes
        {
            bail!("History transport exceeded original immutable aggregate budget");
        }
        Ok(())
    }
    fn remaining(&self, cancel: &CancellationToken) -> Result<Duration> {
        self.check(cancel)?;
        self.limits
            .max_duration
            .checked_sub(self.started.elapsed())
            .filter(|time| !time.is_zero())
            .context("Original history deadline exhausted")
    }
    fn source(
        &self,
        pages: usize,
        rows: usize,
        cancel: &CancellationToken,
    ) -> Result<MetricsReadLimits> {
        let bytes = self
            .limits
            .max_charged_bytes
            .saturating_sub(self.usage.charged_bytes)
            .min(self.limits.source_bytes);
        if bytes == 0 {
            bail!("Original aggregate capture budget exhausted before another gh child");
        }
        Ok(MetricsReadLimits {
            max_pages: pages,
            page_size: self.limits.page_size.min(rows),
            max_rows: rows,
            max_document_bytes: self.limits.document_bytes,
            max_response_bytes: bytes,
            max_duration: self.remaining(cancel)?,
        })
    }
    pub fn scope_limits(
        &self,
        reads: usize,
        cancel: &CancellationToken,
    ) -> Result<MetricsReadLimits> {
        self.check(cancel)?;
        if self.usage.requests.saturating_add(reads) > self.limits.max_requests {
            bail!("Original request cap exhausted before complete canonical scope proof");
        }
        self.source(1, self.limits.page_size, cancel)
    }
    pub fn history_limits(&self, cancel: &CancellationToken) -> Result<MetricsReadLimits> {
        self.check(cancel)?;
        let requests = self
            .limits
            .max_requests
            .saturating_sub(self.usage.requests)
            .saturating_sub(self.closing_requests);
        let pages = (self
            .limits
            .max_pages
            .saturating_sub(self.usage.requested_pages)
            / 2)
        .min(requests.saturating_sub(6) / 2)
        .min(self.limits.source_pages);
        let rows = (self.limits.max_raw_rows.saturating_sub(self.usage.raw_rows) / 2)
            .min(self.limits.source_rows);
        if pages == 0 || rows < self.limits.page_size {
            bail!("Global two-category page/raw-row/request budget cannot fit another history traversal");
        }
        self.source(pages, rows, cancel)
    }
    pub fn charge(&mut self, value: &MetricsReadAccounting) -> Result<()> {
        fn add(current: &mut usize, value: usize) -> Result<()> {
            *current = current
                .checked_add(value)
                .context("History aggregate accounting overflow")?;
            Ok(())
        }
        add(&mut self.usage.requests, value.requests())?;
        add(&mut self.usage.requested_pages, value.requested_pages())?;
        add(&mut self.usage.fetched_pages, value.fetched_pages())?;
        add(&mut self.usage.raw_rows, value.raw_rows())?;
        add(
            &mut self.usage.successful_stdout_bytes,
            value.successful_stdout_bytes(),
        )?;
        add(
            &mut self.usage.successful_diagnostic_bytes,
            value.successful_diagnostic_bytes(),
        )?;
        add(
            &mut self.usage.failed_reserved_bytes,
            value.failed_reserved_bytes(),
        )?;
        add(&mut self.usage.charged_bytes, value.charged_bytes())?;
        if self.usage.requests > self.limits.max_requests
            || self.usage.requested_pages > self.limits.max_pages
            || self.usage.raw_rows > self.limits.max_raw_rows
            || self.usage.charged_bytes > self.limits.max_charged_bytes
        {
            bail!("Opaque actual source accounting exceeded original global allocation");
        }
        Ok(())
    }
    pub fn failed_group(
        &mut self,
        max_requests: usize,
        allocated_capture_bytes: usize,
    ) -> Result<()> {
        // An error exposes no authentic receipt/counters. Charge the whole
        // original group allocation, never derive bytes from sanitized error text.
        self.usage.requests = self
            .usage
            .requests
            .checked_add(max_requests)
            .context("Failed history request reservation overflow")?;
        self.usage.failed_reserved_bytes = self
            .usage
            .failed_reserved_bytes
            .checked_add(allocated_capture_bytes)
            .context("Failed history capture reservation overflow")?;
        self.usage.charged_bytes = self
            .usage
            .charged_bytes
            .checked_add(allocated_capture_bytes)
            .context("Failed history global capture overflow")?;
        Ok(())
    }
    pub fn usage(&self) -> MetricsHistoryUsage {
        self.usage.clone()
    }
}
