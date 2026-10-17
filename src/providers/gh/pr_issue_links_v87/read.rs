//! Fixed original read roles only. No endpoint/query/provider getter escapes.
use super::*;

type InspectionWitnessV87 =
    crate::pr_write_journal::issue_link_origin_v87::history_v87::IssueLinkRecoveryWitnessV87;
enum SelectedV87<'a> {
    Choice {
        catalogue: &'a IssueLinkCatalogueV87,
        target: &'a IssueLinkTargetV87,
    },
    Inspection(&'a InspectionWitnessV87),
}
impl SelectedV87<'_> {
    fn repository(&self) -> &str {
        match self {
            Self::Choice { target, .. } => &target.repository.name,
            Self::Inspection(v) => v.selected_issue_v87().0,
        }
    }
    fn node(&self) -> &str {
        match self {
            Self::Choice { target, .. } => &target.node,
            Self::Inspection(v) => v.selected_issue_v87().3,
        }
    }
    fn number(&self) -> u64 {
        match self {
            Self::Choice { target, .. } => target.number,
            Self::Inspection(v) => v.selected_issue_v87().5,
        }
    }
    fn retained(&self) -> usize {
        match self {
            Self::Choice { catalogue, .. } => catalogue.retained,
            Self::Inspection(_) => 64 * 1024,
        }
    }
    fn matches_repository(&self, raw: &Value) -> GithubResult<Option<(bool, bool)>> {
        match self {
            Self::Choice { target, .. } => target.repository.matches_rest(raw).map(|()| None),
            Self::Inspection(witness) => {
                let (name, id, node, _, _, _) = witness.selected_issue_v87();
                if protocol::positive(&raw["id"])? != id
                    || raw["node_id"] != node
                    || raw["full_name"]
                        .as_str()
                        .is_none_or(|n| !n.eq_ignore_ascii_case(name))
                {
                    return Err(refused(GithubErrorKind::IdentityChanged));
                }
                let archived = raw["archived"]
                    .as_bool()
                    .ok_or_else(|| refused(GithubErrorKind::Json))?;
                let permissions = raw
                    .get("permissions")
                    .and_then(Value::as_object)
                    .ok_or_else(|| refused(GithubErrorKind::Permission))?;
                let permitted =
                    ["push", "maintain", "admin"]
                        .into_iter()
                        .try_fold(false, |any, key| {
                            permissions
                                .get(key)
                                .and_then(Value::as_bool)
                                .map(|v| any || v)
                                .ok_or_else(|| refused(GithubErrorKind::Permission))
                        })?;
                // Two actual REST facts survive the following pair parse; no
                // GraphQL repository/permission is fabricated from a record.
                Ok(Some((archived, permitted)))
            }
        }
    }
    fn parse_pair(
        &self,
        raw: &Value,
        opening: &IssueLinkParentV87,
        host: &str,
        heap: &mut IssueLinkHeapV87,
    ) -> GithubResult<IssueLinkPairV87> {
        match self {
            Self::Choice { target, .. } => {
                IssueLinkPairV87::parse(raw, opening, target, host, heap)
            }
            Self::Inspection(witness) => {
                protocol::error_free(raw)?;
                let target = IssueLinkTargetV87::parse(&raw["data"]["issue"], host, heap)?;
                let (name, repo_id, repo_node, node, id, number) = witness.selected_issue_v87();
                if target.repository.name != name
                    || target.repository.id != repo_id
                    || target.repository.node != repo_node
                    || target.node != node
                    || target.id != id
                    || target.number != number
                {
                    return Err(refused(GithubErrorKind::IdentityChanged));
                }
                IssueLinkPairV87::parse(raw, opening, &target, host, heap)
            }
        }
    }
}
struct SelectedReadV87<'a> {
    current: &'a CurrentCatalogueRepository,
    intent: &'a PrParentIntentV13,
    identity: &'a HostIdentity,
    command: IssueLinkCommandV87,
    purpose: IssueLinkReadPurposeV87,
    retained: Option<&'a OriginalIssueLinksV87>,
    selected: SelectedV87<'a>,
    inherited: usize,
}
pub(crate) struct IssueLinksInspectionReadV87<'a> {
    pub(crate) current: &'a CurrentCatalogueRepository,
    pub(crate) intent: &'a PrParentIntentV13,
    pub(crate) identity: &'a HostIdentity,
    pub(crate) witness: &'a InspectionWitnessV87,
    pub(crate) metadata: bool,
    pub(crate) inherited: usize,
}
impl IssueLinkChoiceV87 {
    fn actual_catalogue(&self) -> GithubResult<Arc<IssueLinkCatalogueV87>> {
        self.catalogue
            .upgrade()
            .filter(|c| self.index < c.rows.len())
            .ok_or_else(|| refused(GithubErrorKind::IdentityChanged))
    }
}
pub(super) fn query(
    provider: &GithubProvider,
    query: &str,
    variables: Value,
    cap: usize,
    binding: &IssueLinkBindingV87,
    heap: &mut IssueLinkHeapV87,
    cancel: &CancellationToken,
) -> GithubResult<bounded::IssueDocumentV87> {
    binding.validate_live(provider, cancel)?;
    let reservation = heap.reserve_document(cap)?;
    // Fixed query text + bounded canonical IDs/cursors. This packet's maximum
    // 16KiB Value/encoded/relocation overlap precedes json/Vec construction.
    heap.charge(3 * 16 * 1024)?;
    let input = json!({"query":query,"variables":variables});
    let bytes = serde_json::to_vec(&input).map_err(|_| refused(GithubErrorKind::Json))?;
    if bytes.len() > 16 * 1024 || bytes.capacity() > 32 * 1024 {
        return Err(refused(GithubErrorKind::Transport));
    }
    let mut spec = provider.spec(vec![
        "api".into(),
        "graphql".into(),
        "--hostname".into(),
        provider.host.clone(),
        "--method".into(),
        "POST".into(),
        "--input".into(),
        "-".into(),
    ]);
    spec.output_limit = spec.output_limit.min(cap);
    spec.stdin = bytes;
    let output = provider
        .runner
        .run(spec, cancel)
        .map_err(|failure| refused(GithubError::from(failure).kind))?;
    binding.validate_live(provider, cancel)?;
    heap.decode(reservation, &output.stdout)
}
pub(super) fn repository(
    provider: &GithubProvider,
    name: &str,
    binding: &IssueLinkBindingV87,
    heap: &mut IssueLinkHeapV87,
    cancel: &CancellationToken,
) -> GithubResult<bounded::IssueDocumentV87> {
    validate_repo(name)?;
    binding.validate_live(provider, cancel)?;
    let reservation = heap.reserve_document(SCALAR_CAP_V87)?;
    let mut spec = provider.spec(vec![
        "api".into(),
        format!("repos/{name}"),
        "--hostname".into(),
        provider.host.clone(),
        "--method".into(),
        "GET".into(),
    ]);
    spec.output_limit = spec.output_limit.min(SCALAR_CAP_V87);
    let output = provider
        .runner
        .run(spec, cancel)
        .map_err(|failure| refused(GithubError::from(failure).kind))?;
    binding.validate_live(provider, cancel)?;
    heap.decode(reservation, &output.stdout)
}

pub(crate) struct IssueLinksSelectedReadV87<'a> {
    pub(crate) current: &'a CurrentCatalogueRepository,
    pub(crate) intent: &'a PrParentIntentV13,
    pub(crate) identity: &'a HostIdentity,
    pub(crate) command: IssueLinkCommandV87,
    pub(crate) choice: &'a IssueLinkChoiceV87,
    pub(crate) purpose: IssueLinkReadPurposeV87,
    pub(crate) retained: Option<&'a OriginalIssueLinksV87>,
}
pub(crate) struct IssueLinksOpenReadV87<'a> {
    pub(crate) current: &'a CurrentCatalogueRepository,
    pub(crate) intent: &'a PrParentIntentV13,
    pub(crate) identity: &'a HostIdentity,
    pub(crate) command: IssueLinkCommandV87,
    pub(crate) repository: Option<&'a IssueLinkRepositoryChoiceV87>,
    pub(crate) max_pages: usize,
}
impl GithubProvider {
    /// Open roles3..(7+P); original Core supplies the surrounding5 boundary
    /// children. Each complete page corroborates the selected repository.
    #[cfg(test)]
    pub(crate) fn original_issue_link_catalogue_v87(
        &self,
        read: IssueLinksOpenReadV87<'_>,
        cancel: &CancellationToken,
    ) -> GithubResult<Arc<IssueLinkCatalogueV87>> {
        match self.original_issue_link_catalogue_native_v102(read, cancel)? {
            IssueCatalogueResultV102::Ready(catalogue) => Ok(catalogue),
            IssueCatalogueResultV102::Unsupported(_) => Err(refused(GithubErrorKind::Unsupported)),
        }
    }
    pub(crate) fn original_issue_link_catalogue_native_v102(
        &self,
        read: IssueLinksOpenReadV87<'_>,
        cancel: &CancellationToken,
    ) -> GithubResult<IssueCatalogueResultV102> {
        if !(1..=4).contains(&read.max_pages)
            || !read.identity.same_account(read.intent.identity())
            || read.identity.host != self.host
            || read
                .identity
                .account_id
                .is_none_or(|n| n == 0 || n > i64::MAX as u64)
        {
            return Err(refused(GithubErrorKind::InvalidTarget));
        }
        let mut heap = IssueLinkHeapV87::new(0)?;
        identity_bound_v87(read.identity)?;
        let binding = IssueLinkBindingV87::capture(self, cancel, &mut heap)?;
        heap.charge(128 * 1024 + 2 * std::mem::size_of::<IssueLinkCatalogueV87>())?;
        let generation = self
            .runner
            .review_resource_generation_v11(
                &self.resource(read.intent.repository(), read.intent.number()),
                cancel,
                binding.deadline,
            )
            .map_err(GithubError::from)?;
        let opening =
            self.original_issue_link_parent_v87(read.current, read.intent, &mut heap, cancel)?;
        let selected_catalogue = read
            .repository
            .map(|choice| {
                let catalogue = choice
                    .catalogue
                    .upgrade()
                    .ok_or_else(|| refused(GithubErrorKind::IdentityChanged))?;
                if !catalogue.identity().same_account(read.identity) {
                    return Err(refused(GithubErrorKind::IdentityChanged));
                }
                let row = catalogue
                    .repositories()
                    .get(choice.index)
                    .ok_or_else(|| refused(GithubErrorKind::IdentityChanged))?;
                if row
                    .node_id
                    .as_ref()
                    .is_none_or(|n| n.is_empty() || n.len() > 1024)
                    || row.id == 0
                    || row.full_name.len() > 256
                {
                    return Err(refused(GithubErrorKind::IdentityChanged));
                }
                Ok(catalogue)
            })
            .transpose()?;
        let (repository_name, repository_id, repository_node) =
            match (read.repository, &selected_catalogue) {
                (Some(choice), Some(catalogue)) => {
                    let row = &catalogue.repositories()[choice.index];
                    (
                        row.full_name.as_str(),
                        row.id,
                        row.node_id
                            .as_deref()
                            .ok_or_else(|| refused(GithubErrorKind::IdentityChanged))?,
                    )
                }
                (None, None) => (
                    opening.parent().repository(),
                    opening.parent().repository_id(),
                    opening.parent().repository_node(),
                ),
                _ => return Err(refused(GithubErrorKind::IdentityChanged)),
            };
        validate_repo(repository_name)?;
        let (owner, name) = repository_name
            .split_once('/')
            .ok_or_else(|| refused(GithubErrorKind::InvalidTarget))?;
        let capability_doc = query(
            self,
            protocol::CAPABILITY_QUERY_V87,
            json!({}),
            SCALAR_CAP_V87,
            &binding,
            &mut heap,
            cancel,
        )?;
        let capability = IssueLinkCapabilityV87::parse(capability_doc.value(), &mut heap)?;
        heap.release_document(capability_doc)?;
        if !capability.supported() {
            // Exact parsed schema-unavailable outcome, not a version or a
            // permission assertion. No private model/DTO crosses this branch.
            if self
                .runner
                .review_resource_generation_v11(
                    &self.resource(read.intent.repository(), read.intent.number()),
                    cancel,
                    binding.deadline,
                )
                .map_err(GithubError::from)?
                != generation
            {
                return Err(refused(GithubErrorKind::IdentityChanged));
            }
            binding.validate_live(self, cancel)?;
            return Ok(IssueCatalogueResultV102::Unsupported(
                IssueHostUnsupportedV102 { _private: () },
            ));
        }
        let mut rows = Vec::new();
        let mut observed_repository = None;
        let mut complete = false;
        let mut cursor: Option<String> = None;
        let mut seen_cursors = std::collections::BTreeSet::new();
        let mut seen_nodes = std::collections::BTreeSet::new();
        let mut seen_ids = std::collections::BTreeSet::new();
        if capability.supported() {
            // Maximum200 actual rows; vector layouts plus relocation debit is
            // reserved before capacity creation, not inferred from wire bytes.
            heap.charge(400 * std::mem::size_of::<IssueLinkTargetV87>() + 64 * 1024)?;
            rows = Vec::with_capacity(200);
            if rows.capacity() > 200 {
                return Err(refused(GithubErrorKind::Transport));
            }
            for _ in 0..read.max_pages {
                let doc = query(
                    self,
                    &protocol::catalogue_query(),
                    json!({"owner":owner,"name":name,"after":cursor}),
                    SCALAR_CAP_V87,
                    &binding,
                    &mut heap,
                    cancel,
                )?;
                protocol::error_free(doc.value())?;
                let raw_repository = &doc.value()["data"]["repository"];
                let actual_repository = IssueLinkRepositoryV87::parse(raw_repository, &mut heap)?;
                if actual_repository.id != repository_id
                    || actual_repository.node != repository_node
                    || !actual_repository.name.eq_ignore_ascii_case(repository_name)
                    || observed_repository
                        .as_ref()
                        .is_some_and(|old: &IssueLinkRepositoryV87| old != &actual_repository)
                {
                    return Err(refused(GithubErrorKind::IdentityChanged));
                }
                let connection = &raw_repository["issues"];
                let page = connection["nodes"]
                    .as_array()
                    .ok_or_else(|| refused(GithubErrorKind::Json))?;
                if page.len() > 50
                    || connection["totalCount"].as_u64().is_none_or(|n| {
                        n < u64::try_from(rows.len() + page.len()).unwrap_or(u64::MAX)
                    })
                {
                    return Err(refused(GithubErrorKind::Json));
                }
                for raw in page {
                    let target = IssueLinkTargetV87::parse(raw, &self.host, &mut heap)?;
                    if !actual_repository.same_identity(&target.repository) {
                        return Err(refused(GithubErrorKind::IdentityChanged));
                    }
                    heap.charge(2 * 512 + target.node.len() * 2 + std::mem::size_of::<String>())?;
                    if !seen_ids.insert(target.id) || !seen_nodes.insert(target.node.clone()) {
                        return Err(refused(GithubErrorKind::Json));
                    }
                    rows.push(target);
                }
                let next = connection["pageInfo"]["hasNextPage"]
                    .as_bool()
                    .ok_or_else(|| refused(GithubErrorKind::Json))?;
                if next {
                    let after = connection["pageInfo"]["endCursor"]
                        .as_str()
                        .filter(|s| {
                            !s.is_empty() && s.len() <= 1024 && !s.chars().any(char::is_control)
                        })
                        .ok_or_else(|| refused(GithubErrorKind::Json))?;
                    heap.charge(512 + 2 * after.len() + 2 * std::mem::size_of::<String>())?;
                    let after = heap.copy(after, 1024)?;
                    if !seen_cursors.insert(after.clone()) {
                        return Err(refused(GithubErrorKind::Json));
                    }
                    cursor = Some(after);
                } else {
                    if connection["totalCount"].as_u64() != u64::try_from(rows.len()).ok() {
                        return Err(refused(GithubErrorKind::Json));
                    }
                    complete = true;
                }
                observed_repository = Some(actual_repository);
                heap.release_document(doc)?;
                if complete {
                    break;
                }
            }
        }
        let pr_sets = if capability.supported() {
            let doc = query(
                self,
                &protocol::pr_links_query(),
                json!({"pr":opening.parent().node_id()}),
                PAIR_CAP_V87,
                &binding,
                &mut heap,
                cancel,
            )?;
            let sets =
                protocol::IssueLinkPrSetsV87::parse(doc.value(), &opening, &self.host, &mut heap)?;
            heap.release_document(doc)?;
            Some(sets)
        } else {
            None
        };
        if self
            .runner
            .review_resource_generation_v11(
                &self.resource(read.intent.repository(), read.intent.number()),
                cancel,
                binding.deadline,
            )
            .map_err(GithubError::from)?
            != generation
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        let repository =
            observed_repository.ok_or_else(|| refused(GithubErrorKind::Unsupported))?;
        binding.validate_live(self, cancel)?;
        let retained = 2 * std::mem::size_of::<IssueLinkCatalogueV87>()
            + binding.retained
            + 128 * 1024
            + opening.parent().retained_heap_bytes_v2()
            + repository.retained()
            + rows.capacity() * std::mem::size_of::<IssueLinkTargetV87>()
            + rows.iter().map(IssueLinkTargetV87::retained).sum::<usize>()
            + pr_sets
                .as_ref()
                .map_or(0, protocol::IssueLinkPrSetsV87::retained);
        if retained > heap.retained()? {
            return Err(refused(GithubErrorKind::Transport));
        }
        Ok(IssueCatalogueResultV102::Ready(Arc::new(
            IssueLinkCatalogueV87 {
                parent: Arc::clone(opening.parent()),
                opening,
                identity: read.identity.clone(),
                repository,
                rows,
                complete,
                binding,
                retained,
                command: read.command,
                capability,
                pr_sets,
                runner_generation: generation,
            },
        )))
    }
    /// Roles3..10 only; original Core owns version/user/repository before and
    /// closing repository/version/final UID after this fixed producer.
    pub(crate) fn observe_original_issue_links_v87(
        &self,
        read: IssueLinksSelectedReadV87<'_>,
        cancel: &CancellationToken,
    ) -> GithubResult<Arc<OriginalIssueLinksV87>> {
        let catalogue = read.choice.actual_catalogue()?;
        let selected = SelectedV87::Choice {
            catalogue: &catalogue,
            target: &catalogue.rows[read.choice.index],
        };
        if !catalogue.binding.issuer.matches(self)
            || catalogue.identity.host != read.identity.host
            || catalogue.identity.account_id != read.identity.account_id
            || !read.identity.same_account(read.intent.identity())
            || read
                .identity
                .account_id
                .is_none_or(|n| n == 0 || n > i64::MAX as u64)
            || !catalogue
                .parent
                .repository()
                .eq_ignore_ascii_case(read.intent.repository())
            || catalogue.parent.node_id() != read.intent.node_id()
            || catalogue.parent.head_sha() != read.intent.head_sha()
            || catalogue.parent.base_tip_sha() != read.intent.base_sha()
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        self.observe_selected_issue_links_v87(
            SelectedReadV87 {
                current: read.current,
                intent: read.intent,
                identity: read.identity,
                command: read.command,
                purpose: read.purpose,
                retained: read.retained,
                selected,
                inherited: 0,
            },
            cancel,
        )
    }
    pub(crate) fn observe_issue_link_inspection_v87(
        &self,
        read: IssueLinksInspectionReadV87<'_>,
        cancel: &CancellationToken,
    ) -> GithubResult<Arc<OriginalIssueLinksV87>> {
        let command = read
            .witness
            .command()
            .map_err(|_| refused(GithubErrorKind::InvalidTarget))?;
        if !read.identity.same_account(read.intent.identity()) {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        self.observe_selected_issue_links_v87(
            SelectedReadV87 {
                current: read.current,
                intent: read.intent,
                identity: read.identity,
                command,
                purpose: if read.metadata {
                    IssueLinkReadPurposeV87::MetadataInspectionV87
                } else {
                    IssueLinkReadPurposeV87::Inspection
                },
                retained: None,
                selected: SelectedV87::Inspection(read.witness),
                inherited: read.inherited,
            },
            cancel,
        )
    }
    fn observe_selected_issue_links_v87(
        &self,
        read: SelectedReadV87<'_>,
        cancel: &CancellationToken,
    ) -> GithubResult<Arc<OriginalIssueLinksV87>> {
        let selected = read.selected;
        let mut heap = IssueLinkHeapV87::new(
            read.retained
                .map_or(selected.retained(), |p| {
                    p.retained.saturating_add(selected.retained())
                })
                .checked_add(read.inherited)
                .ok_or_else(|| refused(GithubErrorKind::Transport))?,
        )?;
        identity_bound_v87(read.identity)?;
        let binding = IssueLinkBindingV87::capture(self, cancel, &mut heap)?;
        heap.charge(2 * std::mem::size_of::<OriginalIssueLinksV87>() + 64 * 1024)?;
        let resources = [
            self.resource(read.intent.repository(), read.intent.number()),
            self.canonical_resource(&format!(
                "{}/{}/issues/{}",
                self.host,
                selected.repository(),
                selected.number()
            )),
        ];
        if resources[0] == resources[1] {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        let generations = self
            .runner
            .issue_link_generations_v87(&resources, cancel, binding.deadline)
            .map_err(GithubError::from)?;
        let opening =
            self.original_issue_link_parent_v87(read.current, read.intent, &mut heap, cancel)?;
        if let SelectedV87::Inspection(witness) = &selected {
            if !witness.matches_parent_v87(opening.parent(), read.identity) {
                return Err(refused(GithubErrorKind::IdentityChanged));
            }
        }
        let capability_doc = query(
            self,
            protocol::CAPABILITY_QUERY_V87,
            json!({}),
            SCALAR_CAP_V87,
            &binding,
            &mut heap,
            cancel,
        )?;
        let capability = IssueLinkCapabilityV87::parse(capability_doc.value(), &mut heap)?;
        heap.release_document(capability_doc)?;
        let repository_doc = repository(self, selected.repository(), &binding, &mut heap, cancel)?;
        let repository_facts = selected.matches_repository(repository_doc.value())?;
        heap.release_document(repository_doc)?;
        let pair_query = protocol::pair_query();
        let variables = || json!({"pr":opening.parent().node_id(),"issue":selected.node()});
        let first_doc = query(
            self,
            &pair_query,
            variables(),
            PAIR_CAP_V87,
            &binding,
            &mut heap,
            cancel,
        )?;
        let pair = selected.parse_pair(first_doc.value(), &opening, &self.host, &mut heap)?;
        if repository_facts.is_some_and(|(archived, permitted)| {
            archived != pair.issue.repository.archived
                || (pair.issue.repository.write == Some(true) && !permitted)
        }) {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        heap.release_document(first_doc)?;
        let selected_doc = query(
            self,
            &protocol::issue_query(),
            json!({"issue":selected.node()}),
            SCALAR_CAP_V87,
            &binding,
            &mut heap,
            cancel,
        )?;
        protocol::error_free(selected_doc.value())?;
        let closing_issue = IssueLinkTargetV87::parse(
            &selected_doc.value()["data"]["issue"],
            &self.host,
            &mut heap,
        )?;
        if closing_issue != pair.issue {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        heap.release_document(selected_doc)?;
        let repository_doc = repository(self, selected.repository(), &binding, &mut heap, cancel)?;
        pair.issue.repository.matches_rest(repository_doc.value())?;
        heap.release_document(repository_doc)?;
        let closing_doc = query(
            self,
            &pair_query,
            variables(),
            PAIR_CAP_V87,
            &binding,
            &mut heap,
            cancel,
        )?;
        let closing_pair =
            selected.parse_pair(closing_doc.value(), &opening, &self.host, &mut heap)?;
        if !pair.same_observation(&closing_pair) {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        heap.release_document(closing_doc)?;
        self.runner
            .validate_issue_link_generations_v87(&resources, generations, cancel, binding.deadline)
            .map_err(GithubError::from)?;
        binding.validate_live(self, cancel)?;
        let parent = Arc::clone(opening.parent());
        // Completed capture/decoder trees and closing duplicates die here.
        // Record only actual persistent typed allocations, not dead3x scratch
        // as an inherited graph in the next genuine phase. The live phase heap
        // above was monotonic and checked throughout; no allowance was reset.
        let retained = 2 * std::mem::size_of::<OriginalIssueLinksV87>()
            + binding.retained
            + 128 * 1024
            + opening.parent().retained_heap_bytes_v2()
            + 256
            + pair.retained()
            + read.identity.host.capacity()
            + read.identity.login.capacity()
            + read.identity.gh_version.capacity()
            + resources.iter().map(String::capacity).sum::<usize>();
        if retained > heap.retained()? {
            return Err(refused(GithubErrorKind::Transport));
        }
        Ok(Arc::new(OriginalIssueLinksV87 {
            opening,
            parent,
            identity: read.identity.clone(),
            command: read.command,
            pair,
            capability,
            binding,
            resources,
            generations,
            purpose: read.purpose,
            retained,
        }))
    }
    pub(crate) fn close_original_issue_link_catalogue_v87(
        &self,
        catalogue: &IssueLinkCatalogueV87,
        current: &CurrentCatalogueRepository,
        intent: &PrParentIntentV13,
        cancel: &CancellationToken,
    ) -> GithubResult<Arc<PrResourceParentV1>> {
        catalogue.binding.validate_live(self, cancel)?;
        let mut heap = IssueLinkHeapV87::new(catalogue.retained)?;
        let closing = self.original_issue_link_parent_v87(current, intent, &mut heap, cancel)?;
        if !catalogue.opening.same_observation(&closing) {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        self.validate_issue_link_catalogue_generation_v87(
            catalogue,
            cancel,
            catalogue.binding.deadline,
        )?;
        Ok(Arc::clone(closing.parent()))
    }
    pub(crate) fn validate_issue_link_catalogue_generation_v87(
        &self,
        catalogue: &IssueLinkCatalogueV87,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        catalogue.binding.validate_lineage(self, cancel, deadline)?;
        if self
            .runner
            .review_resource_generation_v11(
                &self.resource(catalogue.parent.repository(), catalogue.parent.number()),
                cancel,
                deadline.min(catalogue.binding.deadline),
            )
            .map_err(GithubError::from)?
            != catalogue.runner_generation
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        Ok(())
    }
    pub(crate) fn close_original_issue_links_v87(
        &self,
        actual: &OriginalIssueLinksV87,
        current: &CurrentCatalogueRepository,
        intent: &PrParentIntentV13,
        cancel: &CancellationToken,
    ) -> GithubResult<Arc<PrResourceParentV1>> {
        actual.binding.validate_live(self, cancel)?;
        let mut heap = IssueLinkHeapV87::new(actual.retained)?;
        let closing = self.original_issue_link_parent_v87(current, intent, &mut heap, cancel)?;
        if !actual.opening.same_observation(&closing) {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        self.validate_original_issue_link_generations_v87(actual, cancel, actual.deadline())?;
        Ok(Arc::clone(closing.parent()))
    }
    /// Pure lineage/generation check after a successful actual Core close. It
    /// cannot read from, renew or reopen the deliberately sealed/aborted pool.
    pub(crate) fn validate_original_issue_link_generations_v87(
        &self,
        actual: &OriginalIssueLinksV87,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        actual.binding.validate_lineage(self, cancel, deadline)?;
        self.runner
            .validate_issue_link_generations_v87(
                &actual.resources,
                actual.generations,
                cancel,
                deadline.min(actual.deadline()),
            )
            .map_err(GithubError::from)
    }
}
