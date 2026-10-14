//! Undeclared exact branch lookup, never an enumeration/absence proof.
use super::*;
use std::fmt::Write;

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 1024
        && value != "@"
        && value != "HEAD"
        && !value.starts_with('-')
        && !value.starts_with('/')
        && !value.ends_with('/')
        && !value.ends_with('.')
        && !value.contains("..")
        && !value.contains("@{")
        && !value
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || "~^:?*[\\".contains(c))
        && value
            .split('/')
            .all(|part| !part.is_empty() && !part.starts_with('.') && !part.ends_with(".lock"))
}
fn encoded(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            output.push(byte as char);
        } else {
            write!(output, "%{byte:02X}").expect("Writing to String cannot fail");
        }
    }
    output
}
impl GithubProvider {
    /// Exactly eight successful read attempts including one case-sensitive,
    /// percent-encoded raw branch lookup. Expected old tip belongs to the Root
    /// selection, not a public bool/constructor or guessed provider authority.
    pub fn metadata_branch_read(
        &self,
        identity: &HostIdentity,
        original: &PrSummary,
        original_context: &PrContext,
        branch: &str,
        limits: &PrMetadataReadLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<PrMetadataCatalogueRead> {
        let selected = limits.as_metrics()?;
        preflight(self, identity, original, original_context)?;
        if !valid_name(branch) {
            return Err(error(GithubErrorKind::InvalidTarget,"Exact branch requires bounded raw Git name, no controls/checkout shorthand/normalization"));
        }
        let mut budget = MetricsBudget::new(&selected)?;
        let requester = principal(self, &mut budget, identity, cancel)?;
        let name = original.pr.repository.to_ascii_lowercase();
        let repo_endpoint = format!("repos/{name}");
        let parent_endpoint = format!("{repo_endpoint}/pulls/{}", original.pr.number);
        let repository_raw = budget.read_document(self, &repo_endpoint, cancel)?;
        let canonical_repo = repository(&self.host, &name, &repository_raw)?;
        if canonical_repo.id != original_context.repository.id {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Exact branch original repository database identity changed",
            ));
        }
        let parent_raw = budget.read_document(self, &parent_endpoint, cancel)?;
        parent_identity(self, original, &canonical_repo, &parent_raw)?;
        let issue_raw = budget.read_document(
            self,
            &format!("{repo_endpoint}/issues/{}", original.pr.number),
            cancel,
        )?;
        issue_identity(self, original, &issue_raw)?;
        let before_bytes = budget.bytes;
        let mut coverage=MetricsReadCoverage {enumeration_complete:false,metric_evidence_complete:false,complete_empty:false,requested_pages:1,fetched_pages:0,fetched_rows:0,admitted_rows:0,excluded_rows:0,response_bytes:0,reasons:vec!["Exact branch lookup only; repository catalogue not enumerated, unseen/absent branches unknown".into()],status:MetricsReadStatus::Partial};
        let mut rows = vec![];
        match budget.read_document(
            self,
            &format!("{repo_endpoint}/branches/{}", encoded(branch)),
            cancel,
        ) {
            Ok(raw) => {
                coverage.fetched_pages = 1;
                coverage.fetched_rows = 1;
                if raw["name"].as_str().is_some_and(|name| name != branch) {
                    return Err(error(
                        GithubErrorKind::IdentityChanged,
                        "Exact branch response redirected/renamed the case-sensitive raw target",
                    ));
                }
                let size = serde_json::to_vec(&raw)
                    .map_err(|failure| error(GithubErrorKind::Json, failure.to_string()))?
                    .len();
                if raw["name"].as_str() != Some(branch)
                    || !raw.is_object()
                    || size > limits.max_row_bytes
                {
                    coverage.excluded_rows = 1;
                    coverage.reasons.push("Exact branch name/object/row byte proof unavailable; no substitute candidate or empty proof".into());
                } else {
                    if !raw["commit"]["sha"]
                        .as_str()
                        .is_some_and(crate::search::model::valid_sha)
                    {
                        coverage.reasons.push("Actual full branch tip missing/malformed; retained raw row is ineligible, no short SHA expanded".into());
                    }
                    rows.push(raw);
                    coverage.admitted_rows = 1;
                }
            }
            Err(failure) => {
                if matches!(
                    failure.kind,
                    GithubErrorKind::Authentication | GithubErrorKind::IdentityChanged
                ) {
                    return Err(failure);
                }
                coverage.reasons.push(format!(
                    "Exact branch endpoint unavailable: {}",
                    failure.message
                ));
                coverage.status = MetricsReadStatus::Unavailable(failure);
            }
        }
        coverage.response_bytes = budget.bytes - before_bytes;
        let final_repo_raw = budget.read_document(self, &repo_endpoint, cancel)?;
        let final_repo = repository(&self.host, &name, &final_repo_raw)?;
        require_same_repo(&canonical_repo, &final_repo)?;
        let final_parent_raw = budget.read_document(self, &parent_endpoint, cancel)?;
        parent_identity(self, original, &final_repo, &final_parent_raw)?;
        require_same_context(&parent_raw, &final_parent_raw, &issue_raw)?;
        let requester = principal(self, &mut budget, &requester, cancel)?;
        let (target, context, reasons) = observed_context(
            self,
            original,
            &final_repo_raw,
            &final_parent_raw,
            issue_raw,
        )?;
        coverage.reasons.extend(reasons);
        finish(&mut coverage);
        let accounting = PrMetadataReadAccounting {
            requests: budget.requests,
            successful_stdout_bytes: budget.bytes,
            successful_diagnostic_bytes: budget.successful_diagnostic_bytes,
            failed_reserved_bytes: budget.failed_reserved_bytes,
        };
        Ok(PrMetadataCatalogueRead {
            identity: requester,
            target,
            context,
            kind: MetadataChoices::Branches,
            rows,
            coverage,
            repository: final_repo,
            parent_raw: final_parent_raw,
            repository_raw: final_repo_raw,
            observed_interval: (budget.observed_from, budget.observed_through()?),
            accounting,
        })
    }
}
