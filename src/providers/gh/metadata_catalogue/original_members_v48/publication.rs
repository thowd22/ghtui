//! One member invocation and six exact receipt children on the original
//! prepaid cursor. Legacy publication retains its unchanged wrapper.
use super::*;
use crate::pr_metadata_actions::PrMetadataReceipt;

pub(crate) enum PrMetadataMembersFailureV48 {
    NotStarted(GithubError),
    WriteMayHaveStarted(GithubError),
}
pub(crate) struct PrMetadataMembersReceiptV48 {
    actual: Weak<OriginalMetadataMembersV48>,
    plan_sha256: String,
    effect: crate::pr_write_journal::PrConfirmedEffect,
    identity: HostIdentity,
    parent: Arc<PrResourceParentV1>,
    cancellation: CancellationToken,
    deadline: Instant,
    postwrite_v78: Option<MembersPostwriteLineageV78>,
}
/// Read-only continuation of THIS typed receipt, not a new Source or write
/// permit. The original prewrite generation remains mandatory before mutation;
/// only its checked own successor may drive the paid postwrite context.
pub(super) struct MembersPostwriteLineageV78 {
    actual: Weak<OriginalMetadataMembersV48>,
    generation: u64,
    deadline: Instant,
}
impl MembersPostwriteLineageV78 {
    pub(super) fn validate(
        &self,
        provider: &GithubProvider,
        actual: &Arc<OriginalMetadataMembersV48>,
        pool: &Weak<ScopedReadBudgetV1>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        let deadline = deadline.min(self.deadline).min(actual.deadline());
        alive(cancel, deadline)?;
        if !self.actual.ptr_eq(&Arc::downgrade(actual))
            || !actual.metadata_tab_v62()
            || !actual.issuer.matches(provider)
            || !pool.ptr_eq(&Arc::downgrade(&actual.pool))
            || provider
                .scoped_read_budget
                .as_ref()
                .is_some_and(|p| !Arc::ptr_eq(p, &actual.pool))
            || !actual.cancellation.same_instance(cancel)
            || !actual.pool.original_token_matches_v63(cancel)
        {
            return Err(refused(
                GithubErrorKind::IdentityChanged,
                "Actual member postwrite receipt lineage changed",
            ));
        }
        let generation = provider
            .runner
            .review_resource_generation_v11(
                &provider.resource(actual.parent().repository(), actual.parent().number()),
                cancel,
                deadline,
            )
            .map_err(GithubError::from)?;
        if generation != self.generation {
            return Err(refused(
                GithubErrorKind::IdentityChanged,
                "Newer member write retires original postwrite context",
            ));
        }
        Ok(())
    }
}
impl PrMetadataMembersReceiptV48 {
    pub(super) fn postwrite_context_lineage_v78(
        &self,
        provider: &GithubProvider,
        actual: &Arc<OriginalMetadataMembersV48>,
        plan: &PrMetadataPlan,
        cancel: &CancellationToken,
    ) -> GithubResult<MembersPostwriteLineageV78> {
        self.validate_postwrite_context_v78(provider, actual, plan, cancel)?;
        let lineage = self.postwrite_v78.as_ref().ok_or_else(|| {
            refused(
                GithubErrorKind::InvalidTarget,
                "Typed member postwrite lineage missing",
            )
        })?;
        Ok(MembersPostwriteLineageV78 {
            actual: lineage.actual.clone(),
            generation: lineage.generation,
            deadline: lineage.deadline,
        })
    }
    pub(in crate::providers::gh) fn validate_postwrite_context_v78(
        &self,
        provider: &GithubProvider,
        actual: &Arc<OriginalMetadataMembersV48>,
        plan: &PrMetadataPlan,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        if !self.validates_refresh(actual, plan, cancel) {
            return Err(refused(
                GithubErrorKind::IdentityChanged,
                "Actual member receipt/plan changed",
            ));
        }
        self.validate_postwrite_epoch_v78(provider, actual, cancel)
    }
    /// Fixed metadata-only closing check. In particular, after final UID this
    /// does not recreate a Plan, encode JSON, or copy any private response.
    pub(in crate::providers::gh) fn validate_postwrite_epoch_v78(
        &self,
        provider: &GithubProvider,
        actual: &Arc<OriginalMetadataMembersV48>,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        self.postwrite_v78
            .as_ref()
            .ok_or_else(|| {
                refused(
                    GithubErrorKind::InvalidTarget,
                    "Typed member postwrite lineage missing",
                )
            })?
            .validate(
                provider,
                actual,
                &Arc::downgrade(&actual.pool),
                cancel,
                self.deadline,
            )
    }
    pub(in crate::providers::gh) fn validates_metadata_context_v62(
        &self,
        provider: &GithubProvider,
        actual: &Arc<OriginalMetadataMembersV48>,
        plan: &PrMetadataPlan,
        cancel: &CancellationToken,
        pool: &Weak<ScopedReadBudgetV1>,
    ) -> bool {
        actual.metadata_tab_v62()
            && pool.ptr_eq(&Arc::downgrade(&actual.pool))
            && self
                .validate_postwrite_context_v78(provider, actual, plan, cancel)
                .is_ok()
    }
    pub(crate) fn effect(&self) -> &crate::pr_write_journal::PrConfirmedEffect {
        &self.effect
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub(crate) fn parent(&self) -> &Arc<PrResourceParentV1> {
        &self.parent
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }
    pub(crate) fn validates_refresh(
        &self,
        actual: &Arc<OriginalMetadataMembersV48>,
        plan: &PrMetadataPlan,
        cancel: &CancellationToken,
    ) -> bool {
        self.actual.ptr_eq(&Arc::downgrade(actual))
            && self.cancellation.same_instance(cancel)
            && !cancel.is_cancelled()
            && Instant::now() < self.deadline.min(actual.deadline())
            && self.identity.host == actual.identity().host
            && self.identity.account_id == actual.identity().account_id
            && GithubProvider::same_thread_history_parent_v1(self.parent(), actual.parent())
            && plan
                .intent()
                .digest()
                .is_ok_and(|digest| digest == self.plan_sha256)
            && plan
                .receipt()
                .is_ok_and(|receipt| receipt.effect() == self.effect)
    }
}
pub(crate) struct MembersWriteV48<'a> {
    pub(crate) measured: &'a GithubProvider,
    pub(crate) actual: &'a Arc<OriginalMetadataMembersV48>,
    pub(crate) plan: &'a PrMetadataPlan,
    pub(crate) current: &'a CurrentCatalogueRepository,
    pub(crate) captures: &'a mut PostWriteCaptureAllowanceV2,
    pub(crate) cancel: &'a CancellationToken,
    pub(crate) deadline: Instant,
}
/// No serde/Clone/arbitrary-byte constructor. The sole factory below requires
/// the actual live producer and exact current Plan, and drops duplicate Values
/// before the final Core UID callback. Each value consumes exactly ONE slot.
pub(in crate::providers::gh) struct PreparedMembersRequestV48 {
    bytes: Vec<u8>,
    issuer: Arc<ThreadPhaseIssuerV16>,
    cancellation: CancellationToken,
    deadline: Instant,
    resource: Option<String>,
    metadata_v62: bool,
}
impl PreparedMembersRequestV48 {
    pub(in crate::providers::gh) fn metadata_profile_v62(&self) -> bool {
        self.metadata_v62
    }
    pub(in crate::providers::gh) fn prepare_metadata_current_v62(
        actual: &OriginalMetadataMembersV48,
        plan: &PrMetadataPlan,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> GithubResult<Self> {
        actual.require_mutation_v48()?;
        if !actual.metadata_tab_v62()
            || plan.command() != actual.command()
            || plan.pr_id() != actual.parent().database_id()
            || plan.repository_id() != actual.parent().repository_id()
        {
            return Err(refused(
                GithubErrorKind::IdentityChanged,
                "Actual Metadata member readback request differs",
            ));
        }
        let mut query = members_query_v48(plan.command(), plan.selected())?;
        query["variables"]["id"] = json!(actual.parent().node_id());
        Ok(Self {
            bytes: request_bytes_v48(&query)?,
            issuer: Arc::new(ThreadPhaseIssuerV16::capture(provider)?),
            cancellation: cancel.clone(),
            deadline: actual.deadline(),
            resource: None,
            metadata_v62: true,
        })
    }
    pub(in crate::providers::gh) fn deadline_v48(&self) -> Instant {
        self.deadline
    }
    pub(in crate::providers::gh) fn validate_transport_v48(
        &self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        alive(cancel, self.deadline)?;
        if !self.issuer.matches(provider)
            || !self.cancellation.same_instance(cancel)
            || self.bytes.capacity() > 512 * 1024
        {
            return Err(refused(
                GithubErrorKind::IdentityChanged,
                "Prepared actual member request/issuer changed",
            ));
        }
        Ok(())
    }
    pub(in crate::providers::gh) fn into_transport_v48(self) -> (Vec<u8>, Option<String>) {
        (self.bytes, self.resource)
    }
}
struct MembersRequestCounterV48(usize);
impl std::io::Write for MembersRequestCounterV48 {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .filter(|n| *n <= 256 * 1024)
            .ok_or_else(|| std::io::Error::other("Prepared member request exceeds256KiB"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn request_bytes_v48(value: &Value) -> GithubResult<Vec<u8>> {
    let mut counter = MembersRequestCounterV48(0);
    serde_json::to_writer(&mut counter, value)
        .map_err(|e| error(GithubErrorKind::InvalidTarget, e.to_string()))?;
    let mut bytes = Vec::with_capacity(counter.0);
    serde_json::to_writer(&mut bytes, value)
        .map_err(|e| error(GithubErrorKind::Json, e.to_string()))?;
    Ok(bytes)
}
fn prepare_requests_v48(
    actual: &OriginalMetadataMembersV48,
    plan: &PrMetadataPlan,
    current: &CurrentCatalogueRepository,
    provider: &GithubProvider,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<(PreparedMembersRequestV48, PreparedMembersRequestV48)> {
    actual.require_mutation_v48()?;
    plan.validate_original_members_v48(actual)
        .map_err(|message| error(GithubErrorKind::InvalidTarget, message))?;
    if current.repository().id != actual.parent().repository_id()
        || current.repository().node_id.as_deref() != Some(actual.parent().repository_node())
    {
        return Err(refused(
            GithubErrorKind::IdentityChanged,
            "Prepared member current repository changed",
        ));
    }
    let reviews = matches!(plan.command().field(), "reviewers" | "teams");
    let mutation = if reviews {
        publication_actions::PR_METADATA_REQUEST_REVIEWS_MUTATION
    } else {
        publication_actions::PR_METADATA_UPDATE_MUTATION
    };
    let input = plan
        .mutation_input()
        .map_err(|message| error(GithubErrorKind::InvalidTarget, message))?;
    let mutation = json!({"query":mutation,"variables":{"input":input}});
    let mut readback = members_query_v48(plan.command(), plan.selected())?;
    readback["variables"]["id"] = json!(actual.parent().node_id());
    // Preflight encoded lengths before allocating byte copies. The exact
    // field/row caps already bound both Values; unchanged retained whole cap is
    // charged conservatively for the transient Values plus both Vec capacities.
    let mutation_bytes = request_bytes_v48(&mutation)?;
    let readback_bytes = request_bytes_v48(&readback)?;
    drop(mutation);
    drop(readback);
    if mutation_bytes
        .capacity()
        .checked_add(readback_bytes.capacity())
        .is_none_or(|bytes| bytes > 1024 * 1024)
    {
        return Err(refused(
            GithubErrorKind::InvalidTarget,
            "Prepared member retained capacity exceeded",
        ));
    }
    let issuer = Arc::new(ThreadPhaseIssuerV16::capture(provider)?);
    let request = |bytes, resource| PreparedMembersRequestV48 {
        bytes,
        issuer: Arc::clone(&issuer),
        cancellation: cancel.clone(),
        deadline: deadline.min(actual.deadline()),
        resource,
        metadata_v62: actual.metadata_tab_v62(),
    };
    Ok((
        request(
            mutation_bytes,
            Some(provider.resource(actual.parent().repository(), actual.parent().number())),
        ),
        request(readback_bytes, None),
    ))
}
impl GithubProvider {
    pub(crate) fn publish_original_metadata_members_v48(
        &self,
        request: MembersWriteV48<'_>,
        before_write: impl FnOnce() -> GithubResult<HostIdentity>,
    ) -> Result<PrMetadataMembersReceiptV48, PrMetadataMembersFailureV48> {
        let MembersWriteV48 {
            measured: provider,
            actual,
            plan,
            current,
            captures,
            cancel,
            deadline,
        } = request;
        let deadline = deadline.min(actual.deadline()).min(captures.deadline());
        let mut invoked = false;
        let result = (|| {
            alive(cancel, deadline)?;
            actual.require_mutation_v48()?;
            // The runner increments this exact resource once for our fixed
            // mutation. Capture the expected successor BEFORE release, never
            // adopt whichever newer generation happens to exist afterwards.
            let postwrite_generation_v78 = if actual.metadata_tab_v62() {
                Some(actual.runner_generation.checked_add(1).ok_or_else(|| {
                    refused(
                        GithubErrorKind::InvalidTarget,
                        "Member runner generation exhausted before invocation",
                    )
                })?)
            } else {
                None
            };
            plan.validate_original_members_v48(actual)
                .map_err(|message| error(GithubErrorKind::InvalidTarget, message))?;
            captures.validate_prewrite(provider, cancel)?;
            let (pool, _) = measured(provider, cancel)?;
            if !Arc::ptr_eq(&actual.pool, &pool) {
                return Err(refused(
                    GithubErrorKind::IdentityChanged,
                    "Original member pool changed before close",
                ));
            }
            let parent = actual.parent();
            let reviews = matches!(plan.command().field(), "reviewers" | "teams");
            let (mutation_request, members_request) =
                prepare_requests_v48(actual, plan, current, provider, cancel, deadline)?;
            let mut metadata_heap_v62 = if actual.metadata_tab_v62() {
                if captures.reserved_requests() != 22 {
                    return Err(refused(
                        GithubErrorKind::InvalidTarget,
                        "Actual Metadata member paid profile differs",
                    ));
                }
                Some(MetadataMembersHeapV62::new(actual.retained_model_v62()?)?)
            } else {
                None
            };
            let identity = before_write()?;
            alive(cancel, deadline)?;
            if identity.host != actual.identity().host
                || identity.account_id != actual.identity().account_id
            {
                return Err(refused(
                    GithubErrorKind::IdentityChanged,
                    "Closing member numeric actor changed",
                ));
            }
            invoked = true;
            let response = captures.metadata_members_request_v48(
                self,
                mutation_request,
                metadata_heap_v62.as_mut(),
                cancel,
            )?;
            error_free(&response)?;
            if response["data"][if reviews {
                "requestReviews"
            } else {
                "updatePullRequest"
            }]["pullRequest"]["id"]
                .as_str()
                != Some(parent.node_id())
            {
                return Err(refused(
                    GithubErrorKind::AmbiguousWrite,
                    "Member response does not positively identify the exact actual PR",
                ));
            }
            let raw = captures.metadata_members_request_v48(
                self,
                members_request,
                metadata_heap_v62.as_mut(),
                cancel,
            )?;
            error_free(&raw)?;
            members_validation_v48::validate_result_v48(plan, actual.read(), plan.after(), &raw)?;
            let repo_endpoint = format!("repos/{}", parent.repository());
            let repo_raw = if let Some(heap) = metadata_heap_v62.as_mut() {
                captures.members_metadata_role_v62(
                    self,
                    actual,
                    MembersMetadataRoleV62::ReceiptRepository,
                    heap,
                    cancel,
                )?
            } else {
                captures.api(self, &repo_endpoint, "GET", None, None, cancel)?
            };
            let repo = repository(&self.host, parent.repository(), &repo_raw)?;
            require_same_repo(actual.read.canonical_repository(), &repo)?;
            if repo_raw["archived"].as_bool() != Some(false)
                || !["push", "maintain", "admin"]
                    .iter()
                    .any(|key| repo_raw["permissions"][key].as_bool() == Some(true))
            {
                return Err(refused(
                    GithubErrorKind::Permission,
                    "Original member requester write capability unavailable after invocation",
                ));
            }
            let pr_raw = if let Some(heap) = metadata_heap_v62.as_mut() {
                captures.members_metadata_role_v62(
                    self,
                    actual,
                    MembersMetadataRoleV62::ReceiptParent,
                    heap,
                    cancel,
                )?
            } else {
                captures.api(
                    self,
                    &format!("{repo_endpoint}/pulls/{}", parent.number()),
                    "GET",
                    None,
                    None,
                    cancel,
                )?
            };
            let fresh_parent = self.metadata_members_post_parent_v48(current, parent, &pr_raw)?;
            let issue = if let Some(heap) = metadata_heap_v62.as_mut() {
                captures.members_metadata_role_v62(
                    self,
                    actual,
                    MembersMetadataRoleV62::ReceiptIssue,
                    heap,
                    cancel,
                )?
            } else {
                captures.api(
                    self,
                    &format!("{repo_endpoint}/issues/{}", parent.number()),
                    "GET",
                    None,
                    None,
                    cancel,
                )?
            };
            issue_identity(self, parent.summary(), &issue)?;
            require_same_context(&pr_raw, &pr_raw, &issue)?;
            if issue["id"].as_u64() != Some(plan.issue_id())
                || issue["node_id"].as_str() != Some(plan.issue_node())
            {
                return Err(refused(
                    GithubErrorKind::IdentityChanged,
                    "Original member issue database/node changed after invocation",
                ));
            }
            members_validation_v48::validate_rest_result_v48(plan, &pr_raw, &issue)?;
            let user = if let Some(heap) = metadata_heap_v62.as_mut() {
                captures.members_metadata_role_v62(
                    self,
                    actual,
                    MembersMetadataRoleV62::ReceiptUser,
                    heap,
                    cancel,
                )?
            } else {
                captures.api(self, "user", "GET", None, None, cancel)?
            };
            if user["id"].as_u64() != identity.account_id {
                return Err(refused(
                    GithubErrorKind::IdentityChanged,
                    "Actual closing member principal changed",
                ));
            }
            let login = user["login"].as_str().ok_or_else(|| {
                refused(
                    GithubErrorKind::Json,
                    "Actual member requester login unavailable",
                )
            })?;
            validate_name(login)?;
            alive(cancel, deadline)?;
            if captures.consumed_requests() != 6 {
                return Err(refused(
                    GithubErrorKind::InvalidTarget,
                    "Exact six member receipt cursor differs",
                ));
            }
            let receipt = plan
                .receipt()
                .map_err(|message| error(GithubErrorKind::Json, message))?;
            PrMetadataReceipt::decode(&receipt.effect())
                .map_err(|message| error(GithubErrorKind::Json, message))?;
            let postwrite_v78 =
                postwrite_generation_v78.map(|generation| MembersPostwriteLineageV78 {
                    actual: Arc::downgrade(actual),
                    generation,
                    deadline,
                });
            if let Some(lineage) = &postwrite_v78 {
                lineage.validate(
                    self,
                    actual,
                    &Arc::downgrade(&actual.pool),
                    cancel,
                    deadline,
                )?;
            }
            Ok(PrMetadataMembersReceiptV48 {
                actual: Arc::downgrade(actual),
                plan_sha256: plan
                    .intent()
                    .digest()
                    .map_err(|e| error(GithubErrorKind::InvalidTarget, e.to_string()))?,
                effect: receipt.effect(),
                identity: HostIdentity {
                    login: login.into(),
                    ..identity
                },
                parent: Arc::new(fresh_parent),
                cancellation: cancel.clone(),
                deadline,
                postwrite_v78,
            })
        })();
        result.map_err(|failure| {
            if invoked {
                self.runner.mark_ambiguous(
                    &self.resource(actual.parent.repository(), actual.parent.number()),
                );
                PrMetadataMembersFailureV48::WriteMayHaveStarted(failure)
            } else {
                PrMetadataMembersFailureV48::NotStarted(failure)
            }
        })
    }
}
