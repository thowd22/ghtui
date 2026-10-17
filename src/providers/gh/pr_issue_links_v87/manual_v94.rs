//! NEW explicit original read, never a revival of a paid receipt/pool. The
//! only target descriptor comes from the actual typed terminal/OwnFence.
use super::*;
use crate::pr_write_journal::conversation_refresh_v19::IssueLinkManualDescriptorV94;
use crate::providers::gh::ConversationSummaryWitnessV19;

pub(crate) struct IssueLinkManualReadV94<'a> {
    descriptor: &'a IssueLinkManualDescriptorV94,
    binding: IssueLinkBindingV87,
    opening: IssueLinkParentV87,
    summary: Option<ConversationRawSummaryV19>,
    witness: ConversationSummaryWitnessV19,
    pair: Option<IssueLinkPairV87>,
    resources: [String; 2],
    generations: [u64; 2],
    heap: IssueLinkHeapV87,
    finished: bool,
}
impl IssueLinkManualReadV94<'_> {
    pub(crate) fn metadata_tab_v94(&self) -> bool {
        self.descriptor
            .terminal()
            .source_v19()
            .opening()
            .issue_link_metadata_tab_v87()
    }
    pub(crate) fn take_summary_v94(&mut self) -> GithubResult<ConversationRawSummaryV19> {
        self.summary
            .take()
            .ok_or_else(|| refused(GithubErrorKind::InvalidTarget))
    }
    pub(in crate::providers::gh) fn context_existing_v94(&self) -> GithubResult<usize> {
        self.heap.retained()
    }
    pub(in crate::providers::gh) fn validate_live_v94(
        &self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        self.binding.validate_live(provider, cancel)?;
        provider
            .runner
            .validate_issue_link_generations_v87(
                &self.resources,
                self.generations,
                cancel,
                self.binding.deadline,
            )
            .map_err(GithubError::from)
    }
    // Core seals its measured pool BEFORE the final SQL guard/UID. This
    // comparison is used there and after successful close; it is NOT itself
    // a closure proof, and cannot require/revive live-pool read authority.
    pub(crate) fn validate_generation_v94(
        &self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        self.binding
            .validate_lineage(provider, cancel, self.binding.deadline)?;
        if provider
            .scoped_read_budget
            .as_ref()
            .is_none_or(|pool| !Arc::ptr_eq(pool, &self.binding.measured))
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        provider
            .runner
            .validate_issue_link_generations_v87(
                &self.resources,
                self.generations,
                cancel,
                self.binding.deadline,
            )
            .map_err(GithubError::from)
    }
    pub(crate) fn validate_live_before_close_v94(
        &self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        self.validate_live_v94(provider, cancel)
    }
    pub(in crate::providers::gh) fn matches_parent_v94(
        &self,
        parent: &PrResourceParentV1,
        raw: &Value,
    ) -> bool {
        GithubProvider::same_thread_history_parent_v1(self.opening.parent(), parent)
            && self.opening.matches_body(raw)
    }
    pub(in crate::providers::gh) fn original_intent_v94(&self) -> &PrParentIntentV13 {
        self.descriptor.terminal().source_v19().opening().parent()
    }
    pub(in crate::providers::gh) fn detail_v94(
        &mut self,
        provider: &GithubProvider,
        list: ConversationDetailListV19,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<Value>> {
        self.validate_live_v94(provider, cancel)?;
        if self.finished {
            return Err(refused(GithubErrorKind::InvalidTarget));
        }
        let reservation = self.heap.reserve_document(PAIR_CAP_V87)?;
        let mut spec = provider.spec(vec![
            "api".into(),
            list.endpoint_v19(provider, self.opening.parent())?,
            "--hostname".into(),
            provider.host.clone(),
            "--method".into(),
            "GET".into(),
            "--paginate".into(),
            "--slurp".into(),
        ]);
        spec.output_limit = spec.output_limit.min(PAIR_CAP_V87);
        let output = provider
            .runner
            .run(spec, cancel)
            .map_err(GithubError::from)?;
        self.validate_live_v94(provider, cancel)?;
        self.heap.detail_pages_v87(reservation, output)
    }
}

impl GithubProvider {
    pub(crate) fn begin_issue_link_manual_read_v94<'a>(
        &self,
        current: &CurrentCatalogueRepository,
        descriptor: &'a IssueLinkManualDescriptorV94,
        identity: &HostIdentity,
        cancel: &CancellationToken,
    ) -> GithubResult<IssueLinkManualReadV94<'a>> {
        identity_bound_v87(identity)?;
        let opening_intent = descriptor.terminal().source_v19().opening().parent();
        if identity.host != opening_intent.identity().host
            || identity.account_id != opening_intent.identity().account_id
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        // Actual terminal <=64KiB + four bounded origin/parser/move terms
        // <=64KiB, all retained through the phase. Requested issuer/spec/env
        // copies are independently charged by binding/query below.
        let mut heap = IssueLinkHeapV87::new(128 * KIB)?;
        heap.charge(2 * std::mem::size_of::<IssueLinkManualReadV94>() + 64 * KIB)?;
        let binding = IssueLinkBindingV87::capture(self, cancel, &mut heap)?;
        let (repository, _, _, issue_node, _, number) = descriptor.selected_issue_v94();
        let resources = [
            self.resource(opening_intent.repository(), opening_intent.number()),
            self.canonical_resource(&format!("{}/{repository}/issues/{number}", self.host)),
        ];
        if resources[0] == resources[1] {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        heap.charge(
            resources
                .iter()
                .try_fold(0usize, |sum, key| sum.checked_add(key.capacity()))
                .ok_or_else(|| refused(GithubErrorKind::Transport))?,
        )?;
        let generations = self
            .runner
            .issue_link_generations_v87(&resources, cancel, binding.deadline)
            .map_err(GithubError::from)?;
        let (opening, summary) =
            self.fresh_issue_link_parent_v94(current, opening_intent, &mut heap, cancel)?;
        if !descriptor.matches_parent_v94(opening.parent(), identity) {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        let doc = read::query(
            self,
            protocol::CAPABILITY_QUERY_V87,
            json!({}),
            SCALAR_CAP_V87,
            &binding,
            &mut heap,
            cancel,
        )?;
        let capability = IssueLinkCapabilityV87::parse(doc.value(), &mut heap)?;
        if !capability.supported() {
            return Err(refused(GithubErrorKind::Unsupported));
        }
        heap.release_document(doc)?;
        let repo = read::repository(self, repository, &binding, &mut heap, cancel)?;
        if !repo.value().is_object()
            || repo
                .value()
                .get("errors")
                .is_some_and(|value| !matches!(value, Value::Array(rows) if rows.is_empty()))
        {
            return Err(refused(GithubErrorKind::Json));
        }
        let expected_repo = descriptor.selected_issue_v94();
        if protocol::positive(&repo.value()["id"])? != expected_repo.1
            || repo.value()["node_id"] != expected_repo.2
            || repo.value()["full_name"]
                .as_str()
                .is_none_or(|name| !name.eq_ignore_ascii_case(expected_repo.0))
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        // REST is kept alive across the actual pair parse so both actual
        // repository permission/archive observations corroborate each other.
        // Release its scratch BEFORE allocating pair scratch; retain bounded
        // scalar facts only, not a cloned raw repository.
        let archived = repo.value()["archived"]
            .as_bool()
            .ok_or_else(|| refused(GithubErrorKind::Json))?;
        let permitted = ["push", "maintain", "admin"]
            .into_iter()
            .try_fold(false, |any, key| {
                repo.value()["permissions"][key].as_bool().map(|v| any || v)
            })
            .ok_or_else(|| refused(GithubErrorKind::Permission))?;
        heap.release_document(repo)?;
        let doc = read::query(
            self,
            &protocol::pair_query(),
            json!({"pr":opening.parent().node_id(),"issue":issue_node}),
            PAIR_CAP_V87,
            &binding,
            &mut heap,
            cancel,
        )?;
        let target =
            IssueLinkTargetV87::parse(&doc.value()["data"]["issue"], &self.host, &mut heap)?;
        let (name, repo_id, repo_node, node, id, number) = descriptor.selected_issue_v94();
        if target.repository.name != name
            || target.repository.id != repo_id
            || target.repository.node != repo_node
            || target.node != node
            || target.id != id
            || target.number != number
            || target.repository.archived != archived
            || (target.repository.write == Some(true) && !permitted)
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        let pair = IssueLinkPairV87::parse(doc.value(), &opening, &target, &self.host, &mut heap)?;
        heap.release_document(doc)?;
        self.runner
            .validate_issue_link_generations_v87(&resources, generations, cancel, binding.deadline)
            .map_err(GithubError::from)?;
        binding.validate_live(self, cancel)?;
        let witness = summary.witness_v19();
        Ok(IssueLinkManualReadV94 {
            descriptor,
            binding,
            opening,
            summary: Some(summary),
            witness,
            pair: Some(pair),
            resources,
            generations,
            heap,
            finished: false,
        })
    }

    pub(crate) fn finish_issue_link_manual_context_v94(
        &self,
        current: &CurrentCatalogueRepository,
        read: &mut IssueLinkManualReadV94,
        snapshot: &ConversationDetailSnapshotV19,
        cancel: &CancellationToken,
    ) -> GithubResult<ActualIssueLinkPageContextV87> {
        read.validate_live_v94(self, cancel)?;
        if read.finished
            || !read.witness.matches_snapshot_v61(snapshot)?
            || !Arc::ptr_eq(read.opening.parent(), snapshot.parent())
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        // Actual converted five-list/summary models stay live through context,
        // closing pair/Issue/repository/PR and finalUID. Charge before reads.
        read.heap.charge(snapshot.metadata_model_bytes_v51()?)?;
        let pair = read
            .pair
            .take()
            .ok_or_else(|| refused(GithubErrorKind::InvalidTarget))?;
        let metadata = if read.metadata_tab_v94() {
            let context =
                self.pr_issue_link_manual_metadata_v94(current, read, snapshot, cancel)?;
            read.heap.charge(context.retained_bytes())?;
            Some(context)
        } else {
            None
        };
        let doc = read::query(
            self,
            &protocol::pair_query(),
            json!({"pr":read.opening.parent().node_id(),"issue":pair.issue.node}),
            PAIR_CAP_V87,
            &read.binding,
            &mut read.heap,
            cancel,
        )?;
        let closing = IssueLinkPairV87::parse(
            doc.value(),
            &read.opening,
            &pair.issue,
            &self.host,
            &mut read.heap,
        )?;
        if !pair.same_observation(&closing) {
            return Err(refused(GithubErrorKind::StaleHead));
        }
        read.heap.release_document(doc)?;
        let doc = read::query(
            self,
            &protocol::issue_query(),
            json!({"issue":pair.issue.node}),
            SCALAR_CAP_V87,
            &read.binding,
            &mut read.heap,
            cancel,
        )?;
        protocol::error_free(doc.value())?;
        let issue =
            IssueLinkTargetV87::parse(&doc.value()["data"]["issue"], &self.host, &mut read.heap)?;
        if issue != pair.issue {
            return Err(refused(GithubErrorKind::StaleHead));
        }
        read.heap.release_document(doc)?;
        let doc = read::repository(
            self,
            &pair.issue.repository.name,
            &read.binding,
            &mut read.heap,
            cancel,
        )?;
        pair.issue.repository.matches_rest(doc.value())?;
        read.heap.release_document(doc)?;
        read.validate_live_v94(self, cancel)?;
        // MOVE the actual six-set models into the context; no duplicate trees,
        // serializer or reconstructed receipt is needed. Their existing full
        // heap debit remains through the closing parent/UID and storage move.
        read.heap
            .charge(2 * std::mem::size_of::<ActualIssueLinkPageContextV87>())?;
        let digest = read.heap.copy(read.descriptor.plan_digest_v94(), 64)?;
        let context = ActualIssueLinkPageContextV87::from_fresh_v94(
            snapshot,
            read.descriptor,
            pair,
            metadata,
            digest,
            read.heap.retained()?,
        )?;
        read.finished = true;
        Ok(context)
    }
    pub(crate) fn close_issue_link_manual_read_v94(
        &self,
        current: &CurrentCatalogueRepository,
        read: &mut IssueLinkManualReadV94,
        cancel: &CancellationToken,
    ) -> GithubResult<Arc<PrResourceParentV1>> {
        read.validate_live_v94(self, cancel)?;
        if !read.finished {
            return Err(refused(GithubErrorKind::InvalidTarget));
        }
        let intent = read.descriptor.terminal().source_v19().opening().parent();
        let (closing, summary) =
            self.fresh_issue_link_parent_v94(current, intent, &mut read.heap, cancel)?;
        if !read.opening.same_observation(&closing) || !read.witness.matches_v19(&summary) {
            return Err(refused(GithubErrorKind::StaleHead));
        }
        read.validate_live_v94(self, cancel)?;
        Ok(Arc::clone(closing.parent()))
    }
}
