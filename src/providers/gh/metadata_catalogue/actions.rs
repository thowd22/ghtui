//! UNDECLARED source-only ID-native single-field writer. Activation belongs to
//! the same joint lease as crate::pr_metadata_actions. No old provider protocol
//! changes or separate authentication client are introduced.
use super::*;
use crate::pr_metadata_actions::{PrMetadataMember, PrMetadataPlan};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub const PR_METADATA_UPDATE_MUTATION: &str =
    "mutation($input:UpdatePullRequestInput!){updatePullRequest(input:$input){pullRequest{id}}}";
pub const PR_METADATA_REQUEST_REVIEWS_MUTATION: &str =
    "mutation($input:RequestReviewsInput!){requestReviews(input:$input){pullRequest{id}}}";

pub fn pr_metadata_members_query(field: &str) -> GithubResult<String> {
    let selected = match field {
        "labels" => "labels(first:100){totalCount pageInfo{hasNextPage} nodes{id name}}",
        "assignees" => "assignees(first:100){totalCount pageInfo{hasNextPage} nodes{id login}}",
        "milestone" => "milestone{id number title}",
        "reviewers" | "teams" => "reviewRequests(first:100){totalCount pageInfo{hasNextPage} nodes{id requestedReviewer{__typename ...on User{id login} ...on Team{id slug organization{login}}}}}",
        _ => return Err(error(GithubErrorKind::InvalidTarget, "Unknown closed metadata field")),
    };
    // Genuine opaque node membership is cross-checked against bounded REST
    // positive IDs. We do not request deprecated GraphQL databaseId fields or
    // synthesize nodes from REST numbers.
    let mut query = String::from("query($id:ID!");
    if field == "teams" {
        query.push_str(",$candidate:ID!,$includeCandidate:Boolean!");
    }
    query.push_str("){node(id:$id){__typename ...on PullRequest{id fullDatabaseId number headRefOid baseRefOid headRefName baseRefName state isDraft repository{id nameWithOwner} ");
    query.push_str(selected);
    query.push_str("}}");
    if field == "teams" {
        query.push_str(" candidate:node(id:$candidate) @include(if:$includeCandidate){__typename ...on Team{id slug organization{login}}}");
    }
    query.push('}');
    Ok(query)
}

struct MutationBudget {
    started: Instant,
    limits: PrMetadataReadLimits,
    accounting: PrMetadataReadAccounting,
    exhausted: bool,
}
fn utc_now() -> GithubResult<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| {
            error(
                GithubErrorKind::Transport,
                "Metadata observation UTC clock unavailable",
            )
        })
}
impl MutationBudget {
    fn fresh_catalogue(
        &mut self,
        provider: &GithubProvider,
        plan: &PrMetadataPlan,
        cancel: &CancellationToken,
    ) -> GithubResult<PrMetadataCatalogueRead> {
        preflight(
            provider,
            plan.identity(),
            plan.original(),
            plan.source().context(),
        )?;
        let from = utc_now()?;
        let requester = self.principal(provider, plan.identity(), cancel)?;
        let name = plan.target().repository().to_ascii_lowercase();
        let endpoint = format!("repos/{name}");
        let repository_raw = self.get(provider, &endpoint, cancel)?;
        let first = repository(&provider.host, &name, &repository_raw)?;
        if first.id != plan.repository_id() || first.node_id != plan.repository_node() {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Original fresh preparation repository was replaced",
            ));
        }
        let parent_endpoint = format!("{endpoint}/pulls/{}", plan.original().pr.number);
        let parent_raw = self.get(provider, &parent_endpoint, cancel)?;
        parent_identity(provider, plan.original(), &first, &parent_raw)?;
        let issue = self.get(
            provider,
            &format!("{endpoint}/issues/{}", plan.original().pr.number),
            cancel,
        )?;
        issue_identity(provider, plan.original(), &issue)?;
        let kind = plan.command().catalogue();
        let (rows, mut coverage) = self.catalogue_pages(provider, &endpoint, kind, cancel)?;
        let final_repo_raw = self.get(provider, &endpoint, cancel)?;
        let final_repo = repository(&provider.host, &name, &final_repo_raw)?;
        require_same_repo(&first, &final_repo)?;
        let final_parent = self.get(provider, &parent_endpoint, cancel)?;
        parent_identity(provider, plan.original(), &final_repo, &final_parent)?;
        require_same_context(&parent_raw, &final_parent, &issue)?;
        let requester = self.principal(provider, &requester, cancel)?;
        let (target, context, reasons) = observed_context(
            provider,
            plan.original(),
            &final_repo_raw,
            &final_parent,
            issue,
        )?;
        if !reasons.is_empty() {
            coverage.metric_evidence_complete = false;
            coverage.reasons.extend(reasons);
        }
        finish(&mut coverage);
        let through = utc_now()?;
        if through < from {
            return Err(error(
                GithubErrorKind::Transport,
                "UTC moved backwards; metadata preparation interval unavailable",
            ));
        }
        self.finish(cancel)?;
        Ok(PrMetadataCatalogueRead {
            identity: requester,
            target,
            context,
            kind,
            rows,
            coverage,
            repository: final_repo,
            parent_raw: final_parent,
            repository_raw: final_repo_raw,
            observed_interval: (from, through),
            accounting: self.accounting,
        })
    }
    fn catalogue_pages(
        &mut self,
        provider: &GithubProvider,
        repo: &str,
        kind: MetadataChoices,
        cancel: &CancellationToken,
    ) -> GithubResult<(Vec<Value>, MetricsReadCoverage)> {
        let path = match kind {
            MetadataChoices::Labels => "labels",
            MetadataChoices::Assignees => "assignees",
            MetadataChoices::Collaborators => "collaborators",
            MetadataChoices::Teams => "teams",
            MetadataChoices::Milestones => "milestones?state=open",
            MetadataChoices::Branches => {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Branch mutation is not in this closed field writer",
                ))
            }
        };
        let mut coverage = MetricsReadCoverage {
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
        };
        let before = self.accounting.successful_stdout_bytes;
        let mut rows = vec![];
        let mut seen: BTreeMap<String, Value> = BTreeMap::new();
        let mut nodes: BTreeMap<String, String> = BTreeMap::new();
        for page in 1..=self.limits.max_pages {
            if coverage.fetched_rows.saturating_add(self.limits.page_size) > self.limits.max_rows {
                coverage.reasons.push("Fixed eligible page/raw-row cap reached before another child; unseen members unknown".into());
                break;
            }
            let endpoint = format!(
                "{repo}/{path}{}per_page={}&page={page}",
                if path.contains('?') { '&' } else { '?' },
                self.limits.page_size
            );
            coverage.requested_pages += 1;
            let raw = match self.get(provider, &endpoint, cancel) {
                Ok(raw) => raw,
                Err(failure) => {
                    if matches!(
                        failure.kind,
                        GithubErrorKind::Authentication | GithubErrorKind::IdentityChanged
                    ) {
                        return Err(failure);
                    }
                    coverage.metric_evidence_complete = false;
                    coverage.reasons.push(
                        "Eligible endpoint unavailable; no unseen choice/absence inferred".into(),
                    );
                    coverage.status = MetricsReadStatus::Unavailable(failure);
                    break;
                }
            };
            let values = raw.as_array().ok_or_else(|| {
                error(
                    GithubErrorKind::Json,
                    "Eligible page is not an actual REST array",
                )
            })?;
            if values.len() > self.limits.page_size
                || values.len() > self.limits.max_rows.saturating_sub(coverage.fetched_rows)
            {
                return Err(error(
                    GithubErrorKind::Json,
                    "Eligible source exceeded its actual requested page/row bound",
                ));
            }
            coverage.fetched_pages += 1;
            coverage.fetched_rows += values.len();
            for value in values {
                if serde_json::to_vec(value)
                    .map_err(|e| error(GithubErrorKind::Json, e.to_string()))?
                    .len()
                    > self.limits.max_row_bytes
                {
                    coverage.excluded_rows += 1;
                    coverage.metric_evidence_complete = false;
                    coverage.reasons.push(
                        "Observed eligible row exceeds its byte cap; excluded not absence".into(),
                    );
                    continue;
                }
                let key = if matches!(kind, MetadataChoices::Milestones) {
                    value["number"].as_u64()
                } else {
                    value["id"].as_u64()
                }
                .filter(|n| *n > 0)
                .map(|n| n.to_string());
                if let Some(key) = key {
                    if let Some(node) = value.get("node_id").filter(|v| !v.is_null()) {
                        let node = node.as_str().filter(|s| valid_opaque(s)).ok_or_else(|| {
                            error(
                                GithubErrorKind::IdentityChanged,
                                "Supplied eligible opaque node malformed",
                            )
                        })?;
                        if nodes
                            .insert(node.into(), key.clone())
                            .is_some_and(|previous| previous != key)
                        {
                            return Err(error(
                                GithubErrorKind::IdentityChanged,
                                "Eligible opaque node maps to contradictory actual positive IDs",
                            ));
                        }
                    }
                    if let Some(old) = seen.get(&key) {
                        if old != value {
                            return Err(error(
                                GithubErrorKind::IdentityChanged,
                                "Repeated eligible canonical identity changed during traversal",
                            ));
                        }
                        continue;
                    }
                    seen.insert(key, value.clone());
                }
                rows.push(value.clone());
            }
            if values.len() < self.limits.page_size {
                coverage.enumeration_complete = true;
                break;
            }
        }
        coverage.response_bytes = self.accounting.successful_stdout_bytes - before;
        coverage.admitted_rows = rows.len();
        if !coverage.enumeration_complete {
            coverage.metric_evidence_complete = false;
            coverage
                .reasons
                .push("Eligible traversal partial; only genuine observed choices available".into());
        }
        coverage.complete_empty = coverage.enumeration_complete
            && coverage.fetched_rows == 0
            && coverage.excluded_rows == 0;
        finish(&mut coverage);
        Ok((rows, coverage))
    }
    fn finish(&self, cancel: &CancellationToken) -> GithubResult<()> {
        if self.exhausted
            || self.started.elapsed() > self.limits.max_duration
            || cancel.is_cancelled()
            || self.accounting.charged_bytes() > self.limits.max_response_bytes
        {
            return Err(error(GithubErrorKind::Transport,"Metadata publication proof exceeded its immutable aggregate deadline/capture/cancellation budget"));
        }
        Ok(())
    }
    fn check(&self, cancel: &CancellationToken) -> GithubResult<(usize, Duration)> {
        let left = self
            .limits
            .max_response_bytes
            .saturating_sub(self.accounting.charged_bytes());
        let time = self
            .limits
            .max_duration
            .saturating_sub(self.started.elapsed());
        if self.exhausted || left == 0 || time.is_zero() || cancel.is_cancelled() {
            return Err(error(GithubErrorKind::Transport, "Original metadata aggregate capture/deadline/cancellation budget exhausted before another child"));
        }
        Ok((left.min(self.limits.max_document_bytes), time))
    }
    fn call(
        &mut self,
        provider: &GithubProvider,
        endpoint: &str,
        body: Option<Value>,
        write: bool,
        resource: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        let (cap, time) = self.check(cancel)?;
        let mut args = vec![
            "api".into(),
            endpoint.into(),
            "--hostname".into(),
            provider.host.clone(),
            "--method".into(),
            if body.is_some() { "POST" } else { "GET" }.into(),
        ];
        let mut spec = if body.is_some() {
            args.extend(["--input".into(), "-".into()]);
            provider.spec(args)
        } else {
            provider.spec(args)
        };
        if let Some(body) = body {
            let input = serde_json::to_vec(&body)
                .map_err(|e| error(GithubErrorKind::Json, e.to_string()))?;
            if input.len() > 64 * 1024 {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Metadata input exceeds its bounded payload budget",
                ));
            }
            spec.stdin = input;
        }
        spec.output_limit = cap;
        spec.timeout = provider.timeout.min(time);
        if write {
            spec = spec.write(provider.canonical_resource(resource));
        }
        self.accounting.requests += 1;
        let output = match provider.runner.run(spec, cancel) {
            Ok(output) => output,
            Err(failure) => {
                self.accounting.failed_reserved_bytes = self
                    .accounting
                    .failed_reserved_bytes
                    .checked_add(cap)
                    .ok_or_else(|| {
                        error(
                            GithubErrorKind::Transport,
                            "Metadata failed-capture accounting overflow",
                        )
                    })?;
                if matches!(
                    failure.kind,
                    FailureKind::OutputLimit | FailureKind::Timeout | FailureKind::Cancelled
                ) {
                    self.exhausted = true;
                }
                return Err(failure.into());
            }
        };
        self.accounting.successful_stdout_bytes = self
            .accounting
            .successful_stdout_bytes
            .checked_add(output.capture.stdout_bytes())
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Transport,
                    "Metadata raw stdout accounting overflow",
                )
            })?;
        self.accounting.successful_diagnostic_bytes = self
            .accounting
            .successful_diagnostic_bytes
            .checked_add(output.capture.stderr_bytes())
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Transport,
                    "Metadata raw diagnostic accounting overflow",
                )
            })?;
        if output.capture.stdout_bytes() != output.stdout.len()
            || output.capture.total_bytes() > cap
            || self.accounting.charged_bytes() > self.limits.max_response_bytes
        {
            self.exhausted = true;
            return Err(error(
                GithubErrorKind::Json,
                "Metadata runner exceeded its shared pre-request capture cap",
            ));
        }
        let raw: Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| error(GithubErrorKind::Json, e.to_string()))?;
        if raw
            .get("errors")
            .is_some_and(|v| !v.is_null() && v.as_array().is_none_or(|a| !a.is_empty()))
        {
            return Err(error(
                GithubErrorKind::Transport,
                "GraphQL returned errors; known partial data is not publication confirmation",
            ));
        }
        Ok(raw)
    }
    fn get(
        &mut self,
        provider: &GithubProvider,
        endpoint: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        self.call(provider, endpoint, None, false, "", cancel)
    }
    fn principal(
        &mut self,
        provider: &GithubProvider,
        expected: &HostIdentity,
        cancel: &CancellationToken,
    ) -> GithubResult<HostIdentity> {
        let raw = self.get(provider, "user", cancel)?;
        let actual = HostIdentity {
            host: provider.host.clone(),
            login: text(&raw, "login"),
            account_id: Some(positive(&raw, "id")?),
            gh_version: expected.gh_version.clone(),
        };
        validate_name(&actual.login)?;
        if actual.host != expected.host
            || actual.account_id != expected.account_id
            || expected.account_id.is_none_or(|id| id == 0)
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Metadata positive numeric principal changed; no readback receipt",
            ));
        }
        Ok(actual)
    }
}

/// This session carries the actual fresh preparation's original clock and
/// counters into the write/readback. No Clone/Deserialize/public constructor.
pub struct PrMetadataPublicationSession {
    plan_digest: String,
    identity: HostIdentity,
    fresh: PrMetadataCatalogueRead,
    budget: MutationBudget,
    used: bool,
}
impl std::fmt::Debug for PrMetadataPublicationSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrMetadataPublicationSession(<single-use original deadline/capture budget>)")
    }
}
pub struct PrMetadataPublished {
    pub(super) identity: HostIdentity,
    pub(super) accounting: PrMetadataReadAccounting,
    pub(super) plan_digest: String,
}
impl std::fmt::Debug for PrMetadataPublished {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrMetadataPublished(<exact genuine original result membership>)")
    }
}
impl PrMetadataPublished {
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn accounting(&self) -> &PrMetadataReadAccounting {
        &self.accounting
    }
    pub(crate) fn matches(&self, plan: &PrMetadataPlan) -> bool {
        plan.intent().digest().is_ok_and(|d| d == self.plan_digest)
    }
}
impl GithubProvider {
    /// N+11 fresh preparation: catalogue N+7, exact connection 1, canonical
    /// repo/PR/principal 3. No marker/write exists until this succeeds.
    pub fn prepare_metadata_publication(
        &self,
        plan: &PrMetadataPlan,
        limits: &PrMetadataReadLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<PrMetadataPublicationSession> {
        limits.as_metrics()?;
        let mut budget = MutationBudget {
            started: Instant::now(),
            limits: limits.clone(),
            accounting: PrMetadataReadAccounting {
                requests: 0,
                successful_stdout_bytes: 0,
                successful_diagnostic_bytes: 0,
                failed_reserved_bytes: 0,
            },
            exhausted: false,
        };
        let fresh = budget.fresh_catalogue(self, plan, cancel)?;
        plan.validate_fresh(&fresh)
            .map_err(|e| error(GithubErrorKind::InvalidTarget, e))?;
        let mut session = PrMetadataPublicationSession {
            plan_digest: plan
                .intent()
                .digest()
                .map_err(|e| error(GithubErrorKind::InvalidTarget, e.to_string()))?,
            identity: fresh.identity().clone(),
            fresh,
            budget,
            used: false,
        };
        session.verify_members(self, plan, plan.before(), cancel)?;
        session.close_scope(self, plan, false, cancel)?;
        session.budget.check(cancel)?;
        Ok(session)
    }
}
impl PrMetadataPublicationSession {
    pub fn accounting(&self) -> &PrMetadataReadAccounting {
        &self.budget.accounting
    }
    pub(crate) fn ready(
        &self,
        plan: &PrMetadataPlan,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        self.budget.check(cancel)?;
        if self.used
            || plan
                .intent()
                .digest()
                .map_err(|e| error(GithubErrorKind::InvalidTarget, e.to_string()))?
                != self.plan_digest
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Metadata session already used or belongs to another immutable request",
            ));
        }
        Ok(())
    }
    fn verify_members(
        &mut self,
        provider: &GithubProvider,
        plan: &PrMetadataPlan,
        expected: &[PrMetadataMember],
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        let query = pr_metadata_members_query(plan.command().field())?;
        let mut variables = json!({"id":plan.original().pr.id});
        if plan.command().field() == "teams" {
            variables["candidate"] = json!(plan
                .selected()
                .ok_or_else(|| error(
                    GithubErrorKind::InvalidTarget,
                    "Selected genuine team node unavailable"
                ))?
                .node());
            variables["includeCandidate"] =
                json!(plan.command() == crate::pr_metadata_actions::PrMetadataCommand::AddTeam);
        }
        let raw = self.budget.call(
            provider,
            "graphql",
            Some(json!({"query":query,"variables":variables})),
            false,
            "",
            cancel,
        )?;
        super::members_validation_v48::validate_result_v48(plan, &self.fresh, expected, &raw)
    }
    fn close_scope(
        &mut self,
        provider: &GithubProvider,
        plan: &PrMetadataPlan,
        after: bool,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        let name = plan.target().repository().to_ascii_lowercase();
        let repo_raw = self
            .budget
            .get(provider, &format!("repos/{name}"), cancel)?;
        let repo = repository(&provider.host, &name, &repo_raw)?;
        require_same_repo(self.fresh.canonical_repository(), &repo)?;
        if repo.id != plan.repository_id()
            || repo_raw["archived"].as_bool() != Some(false)
            || !["push", "maintain", "admin"]
                .iter()
                .any(|p| repo_raw["permissions"][p].as_bool() == Some(true))
        {
            return Err(error(
                GithubErrorKind::Permission,
                "Current requester original repository field capability is unavailable",
            ));
        }
        let parent = self.budget.get(
            provider,
            &format!("repos/{name}/pulls/{}", plan.original().pr.number),
            cancel,
        )?;
        parent_identity(provider, plan.original(), &repo, &parent)?;
        if parent["id"].as_u64() != Some(plan.pr_id())
            || parent["user"]["id"] != self.fresh.parent_raw()["user"]["id"]
            || parent["state"] != self.fresh.parent_raw()["state"]
            || parent["draft"] != self.fresh.parent_raw()["draft"]
            || parent["merged"] != self.fresh.parent_raw()["merged"]
            || parent["head"]["ref"].as_str() != Some(plan.original().head_ref.as_str())
            || parent["base"]["ref"].as_str() != Some(plan.original().base_ref.as_str())
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Original PR database/author/lifecycle/ref changed at closing metadata guard",
            ));
        }
        if !after {
            require_same_context(
                self.fresh.parent_raw(),
                &parent,
                &self.fresh.context().issue_metadata,
            )?;
        } else {
            let issue = self.budget.get(
                provider,
                &format!("repos/{name}/issues/{}", plan.original().pr.number),
                cancel,
            )?;
            issue_identity(provider, plan.original(), &issue)?;
            require_same_context(&parent, &parent, &issue)?;
            if issue["id"].as_u64() != Some(plan.issue_id())
                || issue["node_id"].as_str() != Some(plan.issue_node())
            {
                return Err(error(
                    GithubErrorKind::IdentityChanged,
                    "Original issue metadata database/node identity changed after publication",
                ));
            }
            super::members_validation_v48::validate_rest_result_v48(plan, &parent, &issue)?;
        }
        self.identity = self.budget.principal(provider, &self.identity, cancel)?;
        Ok(())
    }
    /// Called only by the journal-owning controller after its one-shot claim.
    /// One mutation + complete connection + repo/PR/issue/user = 6 commands.
    pub(crate) fn publish(
        &mut self,
        provider: &GithubProvider,
        plan: &PrMetadataPlan,
        cancel: &CancellationToken,
    ) -> GithubResult<PrMetadataPublished> {
        self.ready(plan, cancel)?;
        self.used = true;
        let resource = provider.resource(plan.target().repository(), plan.original().pr.number);
        let result = (|| {
            let input = plan
                .mutation_input()
                .map_err(|e| error(GithubErrorKind::Unsupported, e))?;
            let reviews = matches!(plan.command().field(), "reviewers" | "teams");
            let mutation = if reviews {
                PR_METADATA_REQUEST_REVIEWS_MUTATION
            } else {
                PR_METADATA_UPDATE_MUTATION
            };
            let returned = self.budget.call(
                provider,
                "graphql",
                Some(json!({"query":mutation,"variables":{"input":input}})),
                true,
                &resource,
                cancel,
            )?;
            if returned["data"][if reviews {
                "requestReviews"
            } else {
                "updatePullRequest"
            }]["pullRequest"]["id"]
                .as_str()
                != Some(plan.original().pr.id.as_str())
            {
                return Err(error(
                    GithubErrorKind::AmbiguousWrite,
                    "Mutation response did not positively identify the exact original PR",
                ));
            }
            self.verify_members(provider, plan, plan.after(), cancel)?;
            self.close_scope(provider, plan, true, cancel)?;
            self.budget.finish(cancel)?;
            Ok(PrMetadataPublished {
                identity: self.identity.clone(),
                accounting: self.budget.accounting,
                plan_digest: self.plan_digest.clone(),
            })
        })();
        provider.after_write(&resource, result)
    }
}
