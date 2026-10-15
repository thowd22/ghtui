//! Exact body intent and fresh original-source Preview. No restored reference,
//! current source record or Preview is a publication permit.
use super::*;
use std::sync::atomic::AtomicBool;

pub struct ReviewOperationRequestV3 {
    source: Arc<ScopedReviewOperationSourceV3>,
    body: Arc<str>,
}
impl std::fmt::Debug for ReviewOperationRequestV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReviewOperationRequestV3(<typed original choice/exact body; no approval>)")
    }
}
impl ReviewOperationRequestV3 {
    pub fn new(
        source: Arc<ScopedReviewOperationSourceV3>,
        body: Arc<str>,
    ) -> Result<Arc<Self>, String> {
        if source.needs_new_v12 {
            return Err(
                "Explicit New action is required before preparing any requested effect".into(),
            );
        }
        let action = source.intent().action();
        if body.len() > 65_536
            || action.needs_body() && body.trim().is_empty()
            || !action.needs_body() && !body.is_empty()
        {
            return Err(
                "Exact pending operation requires nonempty bounded text only for Add/Edit".into(),
            );
        }
        draft_v3::encode(&body, None)?;
        Ok(Arc::new(Self { source, body }))
    }
    pub fn source(&self) -> &Arc<ScopedReviewOperationSourceV3> {
        &self.source
    }
    pub fn intent(&self) -> &Arc<ScopedReviewOperationIntentV3> {
        self.source.intent()
    }
    pub fn body(&self) -> &str {
        &self.body
    }
    pub fn draft_key(&self) -> &str {
        self.source.key()
    }
}
pub struct ReviewOperationPreviewV3 {
    request: Arc<ReviewOperationRequestV3>,
    fresh: Arc<ScopedReviewOperationSourceV3>,
    pub(super) started: AtomicBool,
}
impl std::fmt::Debug for ReviewOperationPreviewV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReviewOperationPreviewV3(<fresh closed source; no write permit>)")
    }
}
impl ReviewOperationPreviewV3 {
    pub fn request(&self) -> &Arc<ReviewOperationRequestV3> {
        &self.request
    }
    pub fn source(&self) -> &Arc<ScopedReviewOperationSourceV3> {
        &self.fresh
    }
    pub fn lines(&self) -> &Arc<[String]> {
        self.fresh.lines()
    }
    pub fn draft_key(&self) -> &str {
        self.fresh.key()
    }
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        request: Arc<ReviewOperationRequestV3>,
        provider: &GithubProvider,
        database: &Path,
        images: &PrImmutableFileLimitsV1,
        reviews: &ReviewReadLimitsV2,
        resources: &PrResourceReadLimitsV1,
        access: &RepositoryCatalogueAccessLimits,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Arc<Self>, ScopedReviewFailureV2> {
        clock(cancel, deadline)?;
        let fresh = if let Some(revision) = &request.source.lineage_v12 {
            ScopedReviewOperationSourceV3::read_lineage_v12(
                request.intent().clone(),
                Some(request.body.clone()),
                revision,
                provider,
                database,
                images,
                reviews,
                resources,
                access,
                cancel,
                deadline,
            )?
        } else {
            ScopedReviewOperationSourceV3::read(
                request.intent().clone(),
                request.body.clone(),
                provider,
                database,
                images,
                reviews,
                resources,
                access,
                cancel,
                deadline,
            )?
        };
        if !Arc::ptr_eq(fresh.intent(), request.intent())
            || fresh.key() != request.draft_key()
            || fresh.body() != request.body()
        {
            return Err(refused(
                "Actual operation Preview source/target/body changed; nothing confirmed",
            ));
        }
        if let Some(reason) = fresh.disabled_reason() {
            return Err(refused(reason));
        }
        fresh.origin_seed()?;
        fresh.prepared_write()?;
        clock(cancel, deadline)?;
        let result = Arc::new(Self {
            request,
            fresh,
            started: AtomicBool::new(false),
        });
        clock(cancel, deadline)?;
        Ok(result)
    }
}
