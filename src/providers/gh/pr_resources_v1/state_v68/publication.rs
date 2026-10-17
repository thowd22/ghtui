//! One fixed preencoded Close/Reopen GraphQL request and independent raw REST
//! readback. All input bytes exist BEFORE final Core UID; no retries/head CAS.
use super::*;
use serde::Serialize;

pub(in crate::providers::gh) struct PreparedStateRequestV68 {
    bytes: Vec<u8>,
    issuer: ThreadPhaseIssuerV16,
    cancel: CancellationToken,
    deadline: Instant,
    resource: String,
}
impl PreparedStateRequestV68 {
    fn from_actual(
        actual: &OriginalPrStateV68,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> GithubResult<Self> {
        actual.validate_live_source_v68()?;
        actual.validate_with_v68(provider, cancel)?;
        actual.require()?;
        #[derive(Serialize)]
        struct Variables<'a> {
            id: &'a str,
        }
        #[derive(Serialize)]
        struct Request<'a> {
            query: &'static str,
            variables: Variables<'a>,
        }
        let query=match actual.observed.command {
            PrStateCommand::Close=>"mutation($id:ID!){closePullRequest(input:{pullRequestId:$id}){pullRequest{id fullDatabaseId number headRefOid baseRefOid headRefName baseRefName state isDraft merged repository{id nameWithOwner}}}}",
            PrStateCommand::Reopen=>"mutation($id:ID!){reopenPullRequest(input:{pullRequestId:$id}){pullRequest{id fullDatabaseId number headRefOid baseRefOid headRefName baseRefName state isDraft merged repository{id nameWithOwner}}}}",
            _=>return Err(failure(GithubErrorKind::Unsupported)),
        };
        let request = Request {
            query,
            variables: Variables {
                id: actual.observed.parent.node_id(),
            },
        };
        struct Count(usize);
        impl std::io::Write for Count {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0 = self
                    .0
                    .checked_add(b.len())
                    .filter(|n| *n <= 8192)
                    .ok_or_else(|| std::io::Error::other("Fixed State input exceeds8KiB"))?;
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut count = Count(0);
        serde_json::to_writer(&mut count, &request).map_err(|_| failure(GithubErrorKind::Json))?;
        let mut bytes = Vec::with_capacity(count.0);
        serde_json::to_writer(&mut bytes, &request).map_err(|_| failure(GithubErrorKind::Json))?;
        if bytes.capacity() > 8192 {
            return Err(failure(GithubErrorKind::InvalidTarget));
        }
        let parent = actual.observed.parent();
        Ok(Self {
            bytes,
            issuer: ThreadPhaseIssuerV16::capture(provider)?,
            cancel: cancel.clone(),
            deadline: actual.publication_cutoff_v68(),
            resource: provider.resource(parent.repository(), parent.number()),
        })
    }
    pub(in crate::providers::gh) fn validate_transport_v68(
        &self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        if !self.issuer.matches(provider)
            || !self.cancel.same_instance(cancel)
            || cancel.is_cancelled()
            || Instant::now() >= self.deadline
            || self.bytes.capacity() > 8192
        {
            return Err(failure(GithubErrorKind::IdentityChanged));
        }
        Ok(())
    }
    pub(in crate::providers::gh) fn deadline(&self) -> Instant {
        self.deadline
    }
    pub(in crate::providers::gh) fn into_transport(self) -> (Vec<u8>, String) {
        (self.bytes, self.resource)
    }
}
pub(in crate::providers::gh::pr_resources_v1) struct StateReceiptExpectationV68<'a> {
    actual: &'a OriginalPrStateV68,
}
impl StateReceiptExpectationV68<'_> {
    pub(in crate::providers::gh::pr_resources_v1) fn old(&self) -> &PrResourceParentV1 {
        self.actual.observed.parent()
    }
    pub(in crate::providers::gh::pr_resources_v1) fn state(&self) -> &'static str {
        desired_state_v68(self.actual.observed.command)
    }
}
fn desired_state_v68(command: PrStateCommand) -> &'static str {
    match command {
        PrStateCommand::Close => "closed",
        PrStateCommand::Reopen => "open",
        _ => "",
    }
}
pub(crate) struct StateTransitionV68 {
    old: Arc<PrResourceParentV1>,
    new: Arc<PrResourceParentV1>,
    command: PrStateCommand,
    issue_id: u64,
    issue_node: String,
}
impl StateTransitionV68 {
    pub(crate) fn old(&self) -> &Arc<PrResourceParentV1> {
        &self.old
    }
    pub(crate) fn parent(&self) -> &Arc<PrResourceParentV1> {
        &self.new
    }
    pub(crate) fn command(&self) -> PrStateCommand {
        self.command
    }
    pub(crate) fn issue_id(&self) -> u64 {
        self.issue_id
    }
    pub(crate) fn issue_node(&self) -> &str {
        &self.issue_node
    }
    pub(in crate::providers::gh::pr_resources_v1) fn state(&self) -> &'static str {
        desired_state_v68(self.command)
    }
}
/// State ONLY varies the requested open/closed field. Numeric identities,
/// author/head repository, H/B/refs/counts/draft/merged remain exact. This does
/// not relax ordinary same_parent or Base's separate permitted B/ref changes.
pub(in crate::providers::gh::pr_resources_v1) fn stable_state_parent_v68(
    old: &PrResourceParentV1,
    new: &PrResourceParentV1,
) -> bool {
    old.database_id == new.database_id
        && old.author_account_id == new.author_account_id
        && old.repository_id == new.repository_id
        && old.repository_node == new.repository_node
        && old.head_repository == new.head_repository
        && old.changed_files == new.changed_files
        && old.summary.pr.host == new.summary.pr.host
        && old.summary.pr.id == new.summary.pr.id
        && old.summary.pr.repository == new.summary.pr.repository
        && old.summary.pr.number == new.summary.pr.number
        && old.summary.head_sha == new.summary.head_sha
        && old.summary.base_sha == new.summary.base_sha
        && old.summary.head_ref == new.summary.head_ref
        && old.summary.base_ref == new.summary.base_ref
        && old.summary.pr.draft == new.summary.pr.draft
        && old.summary.merged == new.summary.merged
}
pub(crate) struct StateReceiptV68 {
    reference: crate::pr_write_journal::PrPublicationReference,
    transition: Arc<StateTransitionV68>,
    observed: Weak<PrStateReadV68>,
    pool: Weak<ScopedReadBudgetV1>,
    cancel: CancellationToken,
    deadline: Instant,
}
impl StateReceiptV68 {
    pub(crate) fn validate_commit_v68(
        &self,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        let pool = self
            .pool
            .upgrade()
            .ok_or_else(|| failure(GithubErrorKind::IdentityChanged))?;
        if !self.cancel.same_instance(cancel)
            || cancel.is_cancelled()
            || Instant::now() >= deadline.min(self.deadline).min(pool.deadline())
        {
            return Err(failure(GithubErrorKind::Transport));
        }
        Ok(())
    }
    pub(crate) fn reference(&self) -> &crate::pr_write_journal::PrPublicationReference {
        &self.reference
    }
    pub(crate) fn effect(&self) -> crate::pr_write_journal::PrConfirmedEffect {
        crate::pr_write_journal::PrConfirmedEffect {
            kind: crate::pr_write_journal::PrEffectKind::Lifecycle,
            resource_id: match self.transition.command {
                PrStateCommand::Close => "state-v68/close/closed",
                PrStateCommand::Reopen => "state-v68/reopen/open",
                _ => "unsupported-state",
            }
            .into(),
            node_id: Some(self.transition.parent().node_id().into()),
        }
    }
    pub(crate) fn transition(&self) -> &Arc<StateTransitionV68> {
        &self.transition
    }
    pub(crate) fn parent(&self) -> &Arc<PrResourceParentV1> {
        self.transition.parent()
    }
    pub(crate) fn publication_cutoff_v68(&self) -> Instant {
        self.deadline
    }
    pub(crate) fn validates_refresh(
        &self,
        actual: &OriginalPrStateV68,
        cancel: &CancellationToken,
    ) -> bool {
        self.observed.ptr_eq(&Arc::downgrade(&actual.observed))
            && self.pool.ptr_eq(&Arc::downgrade(&actual.pool))
            && self.cancel.same_instance(cancel)
            && actual.cancel.same_instance(cancel)
            && !cancel.is_cancelled()
            && Instant::now() < self.deadline.min(actual.publication_cutoff_v68())
            && Arc::ptr_eq(self.transition.old(), actual.observed.parent())
            && self.transition.command == actual.observed.command
            && self.transition.issue_id == actual.observed.issue_id
            && self.transition.issue_node == actual.observed.issue_node
    }
}
pub(crate) enum StatePublicationFailureV68 {
    NotStarted(GithubError),
    WriteMayHaveStarted(GithubError),
}
pub(crate) struct StatePublishInputV68<'a> {
    pub(crate) measured: &'a GithubProvider,
    pub(crate) captures: &'a mut PostWriteCaptureAllowanceV2,
    pub(crate) actual: &'a OriginalPrStateV68,
    pub(crate) current: &'a CurrentCatalogueRepository,
    pub(crate) cancel: &'a CancellationToken,
    pub(crate) reference: &'a crate::pr_write_journal::PrPublicationReference,
}
fn mutation_node(actual: &OriginalPrStateV68, raw: &Value) -> GithubResult<()> {
    error_free(raw)?;
    let field = match actual.observed.command {
        PrStateCommand::Close => "closePullRequest",
        PrStateCommand::Reopen => "reopenPullRequest",
        _ => return Err(failure(GithubErrorKind::Unsupported)),
    };
    let node = &raw["data"][field]["pullRequest"];
    let old = actual.observed.parent();
    if node["id"].as_str() != Some(old.node_id())
        || crate::providers::gh::pr::canonical_database_id_v87(&node["fullDatabaseId"])
            != Some(old.database_id())
        || node["number"].as_u64() != Some(old.number())
        || node["repository"]["id"].as_str() != Some(old.repository_node())
        || node["repository"]["nameWithOwner"]
            .as_str()
            .is_none_or(|s| !s.eq_ignore_ascii_case(old.repository()))
        || node["headRefOid"].as_str() != Some(old.head_sha())
        || node["baseRefOid"].as_str() != Some(old.base_tip_sha())
        || node["headRefName"].as_str() != Some(old.summary().head_ref.as_str())
        || node["baseRefName"].as_str() != Some(old.summary().base_ref.as_str())
        || node["state"].as_str()
            != Some(if actual.observed.command == PrStateCommand::Close {
                "CLOSED"
            } else {
                "OPEN"
            })
        || node["isDraft"].as_bool() != Some(old.summary().pr.draft)
        || node["merged"].as_bool() != Some(false)
    {
        return Err(failure(GithubErrorKind::AmbiguousWrite));
    }
    Ok(())
}
impl GithubProvider {
    pub(crate) fn publish_original_state_v68(
        &self,
        input: StatePublishInputV68<'_>,
        before_write: impl FnOnce() -> GithubResult<HostIdentity>,
    ) -> Result<StateReceiptV68, StatePublicationFailureV68> {
        let StatePublishInputV68 {
            measured: provider,
            captures,
            actual,
            current,
            cancel,
            reference,
        } = input;
        let mut invoked = false;
        let result = (|| {
            actual.validate_with_v68(provider, cancel)?;
            actual.require()?;
            captures.validate_prewrite(provider, cancel)?;
            if captures.reserved_requests() != STATE_POST_CAPS_V68.len()
                || captures.reserved_bytes() != STATE_POST_CAPS_V68.iter().sum::<usize>()
                || captures.consumed_requests() != 0
            {
                return Err(failure(GithubErrorKind::InvalidTarget));
            }
            // Exact bytes, endpoint, expectation and all their copies precede
            // final UID. No Value construction/serialization follows close.
            let request = PreparedStateRequestV68::from_actual(actual, provider, cancel)?;
            if reference.attempt_id <= 0
                || reference.observed_head != actual.observed.parent().head_sha()
                || reference.payload_sha256.len() != 64
                || !reference
                    .payload_sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(failure(GithubErrorKind::InvalidTarget));
            }
            // This descriptive reference comes from the SAME actual claimed
            // permit/ACK guard. It grants no claim/invocation by itself.
            let reference = reference.clone();
            let parent = actual.observed.parent();
            let endpoint = format!("repos/{}/pulls/{}", parent.repository(), parent.number());
            let expected = StateReceiptExpectationV68 { actual };
            let identity = before_write()?;
            if !identity.same_account(actual.identity()) {
                return Err(failure(GithubErrorKind::IdentityChanged));
            }
            if cancel.is_cancelled()
                || Instant::now() >= actual.publication_cutoff_v68().min(captures.deadline())
            {
                return Err(failure(GithubErrorKind::Transport));
            }
            invoked = true;
            let raw = captures.state_request_v68(self, request, cancel)?;
            mutation_node(actual, &raw)?;
            drop(raw);
            let raw = captures.api(self, &endpoint, "GET", None, None, cancel)?;
            if raw.get("errors").is_some_and(|v| !v.is_null()) || raw.get("message").is_some() {
                return Err(failure(GithubErrorKind::AmbiguousWrite));
            }
            let fresh = parent_expected_v13(
                self,
                current,
                ExpectedPrParentV13::StateReceiptV68(&expected),
                &raw,
            )?;
            if captures.consumed_requests() != 2 {
                return Err(failure(GithubErrorKind::InvalidTarget));
            }
            Ok(StateReceiptV68 {
                reference,
                transition: Arc::new(StateTransitionV68 {
                    old: Arc::clone(parent),
                    new: Arc::new(fresh),
                    command: actual.observed.command,
                    issue_id: actual.observed.issue_id,
                    issue_node: actual.observed.issue_node.clone(),
                }),
                observed: Arc::downgrade(&actual.observed),
                pool: Arc::downgrade(&actual.pool),
                cancel: cancel.clone(),
                deadline: actual.publication_cutoff_v68().min(captures.deadline()),
            })
        })();
        result.map_err(|e| {
            if invoked {
                let parent = actual.observed.parent();
                self.runner
                    .mark_ambiguous(&self.resource(parent.repository(), parent.number()));
                StatePublicationFailureV68::WriteMayHaveStarted(e)
            } else {
                StatePublicationFailureV68::NotStarted(e)
            }
        })
    }
}
