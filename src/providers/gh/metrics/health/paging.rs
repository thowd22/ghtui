use super::*;

pub(super) fn coverage() -> MetricsReadCoverage {
    MetricsReadCoverage {
        enumeration_complete: false,
        metric_evidence_complete: true,
        complete_empty: false,
        requested_pages: 0,
        fetched_pages: 0,
        fetched_rows: 0,
        admitted_rows: 0,
        excluded_rows: 0,
        response_bytes: 0,
        reasons: vec![],
        status: MetricsReadStatus::Partial,
    }
}
pub(super) fn finish(coverage: &mut MetricsReadCoverage) {
    coverage.complete_empty =
        coverage.enumeration_complete && coverage.fetched_rows == 0 && coverage.excluded_rows == 0;
    coverage.reasons.sort();
    coverage.reasons.dedup();
    if !matches!(coverage.status, MetricsReadStatus::Unavailable(_)) {
        coverage.status = if coverage.enumeration_complete && coverage.metric_evidence_complete {
            MetricsReadStatus::Complete
        } else {
            MetricsReadStatus::Partial
        };
    }
}
pub(super) fn hard_failure(error: &GithubError) -> bool {
    matches!(
        error.kind,
        GithubErrorKind::Authentication
            | GithubErrorKind::IdentityChanged
            | GithubErrorKind::StaleHead
    )
}
pub(super) struct HealthBudget {
    pub transport: HealthTransport,
    limits: MetricsReadLimits,
    pub raw_rows: usize,
    pub collection_pages: usize,
}
impl HealthBudget {
    pub fn new(limits: &MetricsReadLimits) -> GithubResult<Self> {
        Ok(Self {
            transport: HealthTransport::new(limits)?,
            limits: limits.clone(),
            raw_rows: 0,
            collection_pages: 0,
        })
    }
    pub fn principal(
        &mut self,
        provider: &GithubProvider,
        expected: &HostIdentity,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        let current = self.transport.read_document(provider, "user", cancel)?;
        if positive(&current, "id")? != expected.account_id.unwrap_or(0) {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Metric health principal changed; old private evidence is not published",
            ));
        }
        validate_name(&text(&current, "login"))
    }
    pub fn collect<T: Clone + PartialEq>(
        &mut self,
        provider: &GithubProvider,
        base: &str,
        field: &str,
        header: impl Fn(&Value) -> GithubResult<()>,
        mut parse: impl FnMut(&Value) -> GithubResult<(String, T, Vec<String>)>,
        cancel: &CancellationToken,
    ) -> GithubResult<(Vec<T>, MetricsReadCoverage)> {
        let mut values = vec![];
        let mut seen = BTreeMap::<String, T>::new();
        let mut result = coverage();
        let mut total = None;
        let before_bytes = self.transport.bytes;
        for page in 1..=self.limits.max_pages {
            if self.collection_pages >= self.limits.max_pages
                || self.raw_rows.saturating_add(self.limits.page_size) > self.limits.max_rows
            {
                result.metric_evidence_complete = false;
                result.reasons.push(
                    "Shared fixed-page/raw-row budget reached before another health source request"
                        .into(),
                );
                break;
            }
            result.requested_pages += 1;
            self.collection_pages += 1;
            let separator = if base.contains('?') { "&" } else { "?" };
            let endpoint = format!(
                "{base}{separator}per_page={}&page={page}",
                self.limits.page_size
            );
            let value = match self.transport.read(provider, &endpoint, cancel) {
                Ok(value) => value,
                Err(err) if hard_failure(&err) => return Err(err),
                Err(err) => {
                    result.metric_evidence_complete = false;
                    result.reasons.push(err.message.clone());
                    result.status = MetricsReadStatus::Unavailable(err);
                    break;
                }
            };
            if let Err(err) = header(&value) {
                if hard_failure(&err) {
                    return Err(err);
                }
                result.metric_evidence_complete = false;
                result.reasons.push(err.message.clone());
                result.status = MetricsReadStatus::Unavailable(err);
                break;
            }
            let Some(rows) = value[field].as_array() else {
                result.metric_evidence_complete = false;
                result.reasons.push(format!(
                    "Health source omitted actual {field} array; no empty proof"
                ));
                break;
            };
            if rows.len() > self.limits.page_size
                || rows.len() > self.limits.max_rows.saturating_sub(self.raw_rows)
            {
                return Err(error(
                    GithubErrorKind::Json,
                    "Health endpoint exceeded pre-request page/raw-row budget",
                ));
            }
            let current_total = value["total_count"].as_u64();
            if total.is_some() && current_total != total {
                result.metric_evidence_complete = false;
                result.reasons.push("Health total_count moved or disappeared during paging; non-atomic coverage remains partial".into());
            }
            if total.is_none() {
                total = current_total;
            }
            if current_total.is_none() {
                result.metric_evidence_complete = false;
                result.reasons.push("Health total_count omitted; page length alone cannot certify current complete inventory".into());
            }
            result.fetched_pages += 1;
            result.fetched_rows += rows.len();
            self.raw_rows += rows.len();
            for row in rows {
                if serde_json::to_vec(row)
                    .map_err(|err| error(GithubErrorKind::Json, err.to_string()))?
                    .len()
                    > self.limits.max_document_bytes
                {
                    result.excluded_rows += 1;
                    result.metric_evidence_complete = false;
                    result.reasons.push(
                        "Health document exceeds selected bytes; known excluded row is not absence"
                            .into(),
                    );
                    continue;
                }
                let (key, item, reasons) = match parse(row) {
                    Ok(item) => item,
                    Err(err) if hard_failure(&err) => return Err(err),
                    Err(err) => {
                        result.excluded_rows += 1;
                        result.metric_evidence_complete = false;
                        result.reasons.push(err.message);
                        continue;
                    }
                };
                if !reasons.is_empty() {
                    result.metric_evidence_complete = false;
                    result.reasons.extend(reasons);
                }
                if let Some(old) = seen.get(&key) {
                    if old != &item {
                        return Err(error(GithubErrorKind::IdentityChanged,"Repeated genuine health identity changed while paging; no arbitrary state/attempt chosen"));
                    }
                    result.metric_evidence_complete = false;
                    result.reasons.push(
                        "Repeated health record while paging; enumeration is non-atomic/partial"
                            .into(),
                    );
                    continue;
                }
                seen.insert(key, item.clone());
                values.push(item);
            }
            if rows.len() < self.limits.page_size {
                result.enumeration_complete = total
                    .is_some_and(|total| total == result.fetched_rows as u64)
                    && current_total == total;
                if !result.enumeration_complete {
                    result.metric_evidence_complete = false;
                    result.reasons.push("Short health page contradicts or lacks declared total; unseen records remain unknown".into());
                }
                break;
            }
        }
        result.admitted_rows = values.len();
        result.response_bytes = self.transport.bytes - before_bytes;
        if !result.enumeration_complete {
            result.metric_evidence_complete = false;
            result
                .reasons
                .push("Current API health collection not exhausted under requested bounds".into());
        }
        finish(&mut result);
        Ok((values, result))
    }
}
