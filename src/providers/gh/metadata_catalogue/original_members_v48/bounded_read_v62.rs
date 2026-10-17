//! Metadata-Members fixed issue/current children only. Ordinary API calls and
//! their failure bytes remain unchanged; no endpoint or provider API escapes.
use super::*;

/// Actual read-phase data only; no public constructor/raw/provider getter.
pub(crate) struct MembersMetadataHistoryReadV62 {
    parent: Arc<PrResourceParentV1>,
    heap: MetadataMembersHeapV62,
    issuer: ThreadPhaseIssuerV16,
    pool: Arc<ScopedReadBudgetV1>,
    cancellation: CancellationToken,
    deadline: Instant,
}
impl MembersMetadataHistoryReadV62 {
    pub(crate) fn parent(&self) -> &Arc<PrResourceParentV1> {
        &self.parent
    }
}
impl GithubProvider {
    pub(crate) fn begin_members_metadata_history_v62(
        &self,
        current: &CurrentCatalogueRepository,
        intent: &PrParentIntentV13,
        cancel: &CancellationToken,
    ) -> GithubResult<MembersMetadataHistoryReadV62> {
        let (pool, deadline) = measured(self, cancel)?;
        let mut heap = MetadataMembersHeapV62::new(0)?;
        let actual =
            self.metadata_members_bounded_parent_v62(current, intent, &mut heap, cancel)?;
        Ok(MembersMetadataHistoryReadV62 {
            parent: Arc::clone(actual.parent()),
            heap,
            issuer: ThreadPhaseIssuerV16::capture(self)?,
            pool,
            cancellation: cancel.clone(),
            deadline,
        })
    }
    pub(crate) fn close_members_metadata_history_v62(
        &self,
        read: &mut MembersMetadataHistoryReadV62,
        current: &CurrentCatalogueRepository,
        intent: &PrParentIntentV13,
        cancel: &CancellationToken,
    ) -> GithubResult<Arc<PrResourceParentV1>> {
        let (pool, deadline) = measured(self, cancel)?;
        alive(cancel, deadline.min(read.deadline))?;
        if !read.issuer.matches(self)
            || !Arc::ptr_eq(&pool, &read.pool)
            || !read.cancellation.same_instance(cancel)
        {
            return Err(refused(
                GithubErrorKind::IdentityChanged,
                "Actual Metadata member History lineage differs",
            ));
        }
        let actual =
            self.metadata_members_bounded_parent_v62(current, intent, &mut read.heap, cancel)?;
        if !GithubProvider::same_thread_history_parent_v1(&read.parent, actual.parent()) {
            return Err(refused(
                GithubErrorKind::IdentityChanged,
                "Actual Metadata member History parent changed",
            ));
        }
        Ok(Arc::clone(actual.parent()))
    }
}

pub(super) fn issue(
    provider: &GithubProvider,
    parent: &PrResourceParentV1,
    heap: &mut MetadataMembersHeapV62,
    cancel: &CancellationToken,
) -> GithubResult<Value> {
    read(
        provider,
        vec![
            "api".into(),
            format!("repos/{}/issues/{}", parent.repository(), parent.number()),
            "--hostname".into(),
            provider.host.clone(),
            "--method".into(),
            "GET".into(),
        ],
        None,
        heap,
        cancel,
    )
}
pub(super) fn current(
    provider: &GithubProvider,
    parent: &PrResourceParentV1,
    command: PrMetadataCommand,
    selected: Option<&PrMetadataMember>,
    heap: &mut MetadataMembersHeapV62,
    cancel: &CancellationToken,
) -> GithubResult<Value> {
    let mut request = members_query_v48(command, selected)?;
    request["variables"]["id"] = json!(parent.node_id());
    read(
        provider,
        vec![
            "api".into(),
            "graphql".into(),
            "--hostname".into(),
            provider.host.clone(),
            "--method".into(),
            "POST".into(),
            "--input".into(),
            "-".into(),
        ],
        Some(request),
        heap,
        cancel,
    )
}
// Private implementation of the two fixed roles above, not a crate transport
// accessor. SAME actual provider.spec/runner/token/window/measured allocation.
fn read(
    provider: &GithubProvider,
    args: Vec<String>,
    input: Option<Value>,
    heap: &mut MetadataMembersHeapV62,
    cancel: &CancellationToken,
) -> GithubResult<Value> {
    measured(provider, cancel)?;
    let mut spec = provider.spec(args);
    spec.output_limit = spec.output_limit.min(128 * 1024);
    if let Some(input) = input {
        spec.stdin = serde_json::to_vec(&input)
            .map_err(|_| refused(GithubErrorKind::Json, "Actual member query unavailable"))?;
    }
    let output = provider.runner.run(spec, cancel).map_err(|failure| {
        refused(
            GithubError::from(failure).kind,
            "Original Metadata members observation unavailable",
        )
    })?;
    measured(provider, cancel)?;
    heap.object(&output.stdout)
}
