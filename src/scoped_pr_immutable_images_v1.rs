//! SOURCE-ONLY child of scoped resources: original-bound immutable file images.
//! Reuses the existing session/SQL guard/page publisher; no new auth or writer.
use super::*;
use crate::providers::gh::{
    PrImageSideV1, PrImmutableFileLimitsV1, PrImmutableFileOriginalV1, PrImmutableFileReadV1,
};
#[path = "scoped_pr_inline_intent_v1.rs"]
pub mod inline_v1;
#[path = "scoped_pr_immutable_image_native_v1.rs"]
pub mod native;
#[path = "scoped_pr_review_intent_v2.rs"]
pub mod reviews_v2;
#[path = "scoped_pr_immutable_image_ui_v1.rs"]
pub mod ui;

pub struct PrImmutableImageInputV1 {
    input: PrResourceInputV1,
    file_index: usize,
}
impl PrImmutableImageInputV1 {
    pub(crate) fn restore_failed_submission(self, app: &mut crate::app::App) -> bool {
        let Some(github) = app.github.as_mut() else {
            return false;
        };
        if github.pr.is_some()
            || github
                .scoped_pr_navigation
                .as_ref()
                .is_none_or(|nav| !Arc::ptr_eq(nav, &self.input.observed.navigation))
            || github.scope().ok().as_ref() != Some(&self.input.observed.navigation.scope)
        {
            return false;
        }
        github.pr = Some(self.input.observed.page);
        github.scoped_pr_resource = self.input.previous_view;
        true
    }
}
pub fn take_image_input(
    workspace: &mut GithubWorkspace,
    row: usize,
) -> Result<PrImmutableImageInputV1, ObservedPrTabFailureV1> {
    let view = workspace
        .scoped_pr_resource
        .as_ref()
        .ok_or(ObservedPrTabFailureV1::SelectedPageUnavailable)?;
    let file_index = match view
        .row(row)
        .ok_or(ObservedPrTabFailureV1::SelectedPageUnavailable)?
    {
        PrResourceRowV1::FileList(i) => i,
        PrResourceRowV1::File(i) | PrResourceRowV1::Hunk(i, _) | PrResourceRowV1::Line(i, _) => {
            let file = view
                .diff
                .as_ref()
                .and_then(|diff| diff.files.get(i))
                .ok_or(ObservedPrTabFailureV1::SelectedPageUnavailable)?;
            let path = file
                .new_path
                .as_deref()
                .or(file.old_path.as_deref())
                .ok_or(ObservedPrTabFailureV1::SelectedPageUnavailable)?;
            view.observed
                .files()
                .iter()
                .position(|row| row.path.as_bytes() == path)
                .ok_or(ObservedPrTabFailureV1::SelectedPageUnavailable)?
        }
        _ => return Err(ObservedPrTabFailureV1::SelectedPageUnavailable),
    };
    Ok(PrImmutableImageInputV1 {
        input: take_resource_input(workspace, PrResourceKindV1::Files)?,
        file_index,
    })
}
/// Not a current permission lease. This private source proof was issued ONLY
/// after actual original repository/local/final-principal closure, and is retired
/// by the same Source/Scope/Nav+operation owner as the existing selected page.
pub struct ClosedPrFileImagesV1 {
    read: Arc<PrImmutableFileReadV1>,
    original_view: Arc<ScopedPrResourceViewV1>,
    pub(crate) source: Arc<CatalogueIntentSource>,
    pub(crate) active: Arc<ActiveRemoteScopeV1>,
    pub(crate) navigation: Arc<ScopedPrNavigationV1>,
    old_lines: Vec<(usize, usize)>,
    new_lines: Vec<(usize, usize)>,
}
impl std::fmt::Debug for ClosedPrFileImagesV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ClosedPrFileImagesV1(<immutable bytes; read-only; no write approval>)")
    }
}
impl ClosedPrFileImagesV1 {
    /// Conservative owned-observation charge. The pre-existing original Files
    /// view/page/source/navigation Arcs are a separate source budget, not newly
    /// allocated full-side display material and not recursively double charged.
    pub(crate) fn retained_original_observation_bytes_v2(&self) -> usize {
        self.read.retained_content_bytes_v1()
            + (self.old_lines.capacity() + self.new_lines.capacity())
                * std::mem::size_of::<(usize, usize)>()
            + std::mem::size_of::<Self>()
            + 64
    }
    pub fn observed(&self) -> &PrImmutableFileReadV1 {
        &self.read
    }
    pub fn source_scope(&self) -> &Scope {
        self.source.scope()
    }
    pub fn line_count(&self, side: PrImageSideV1) -> usize {
        self.lines(side).len()
    }
    fn lines(&self, side: PrImageSideV1) -> &[(usize, usize)] {
        if side == PrImageSideV1::Old {
            &self.old_lines
        } else {
            &self.new_lines
        }
    }
    pub fn line(&self, side: PrImageSideV1, number: u32) -> Option<&[u8]> {
        let (start, end) = *self.lines(side).get(number.checked_sub(1)? as usize)?;
        self.read.image(side).bytes()?.get(start..end)
    }
    pub fn canonical_range(
        self: &Arc<Self>,
        side: PrImageSideV1,
        first: u32,
        last: u32,
    ) -> Result<ReadOnlyCanonicalRangeV1, String> {
        if first == 0
            || first > last
            || last as usize > self.line_count(side)
            || last - first > 9999
            || !self.read.range_admitted(side, first, last)
        {
            return Err("Selection is outside one actual canonical PR hunk/side or exceeds the read-only range bound".into());
        }
        Ok(ReadOnlyCanonicalRangeV1 {
            proof: self.clone(),
            side,
            first,
            last,
        })
    }
    pub fn file_index(&self) -> usize {
        self.read.file_index()
    }
    pub fn owns(&self, app: &crate::app::App) -> bool {
        app.remote_scope_source
            .as_ref()
            .is_some_and(|source| Arc::ptr_eq(source, &self.source))
            && app
                .remote_scope
                .as_ref()
                .is_some_and(|scope| Arc::ptr_eq(scope, &self.active))
            && app.github.as_ref().is_some_and(|github| {
                github
                    .scoped_pr_navigation
                    .as_ref()
                    .is_some_and(|nav| Arc::ptr_eq(nav, &self.navigation))
                    && github
                        .scoped_pr_resource
                        .as_ref()
                        .is_some_and(|view| Arc::ptr_eq(view, &self.original_view))
                    && github.scope().ok().as_ref() == Some(self.source.scope())
                    && github
                        .pr
                        .as_ref()
                        .is_some_and(|page| matches_page(&self.navigation, page))
            })
    }
}
/// Private canonical READ provenance, not a GitHub write permit or restored
/// approval. LEFT publication, if later authorized, always uses review_head H.
pub struct ReadOnlyCanonicalRangeV1 {
    proof: Arc<ClosedPrFileImagesV1>,
    side: PrImageSideV1,
    first: u32,
    last: u32,
}
impl std::fmt::Debug for ReadOnlyCanonicalRangeV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReadOnlyCanonicalRangeV1")
            .field("side", &self.side)
            .field("first", &self.first)
            .field("last", &self.last)
            .finish_non_exhaustive()
    }
}
impl ReadOnlyCanonicalRangeV1 {
    pub fn side(&self) -> PrImageSideV1 {
        self.side
    }
    pub fn start(&self) -> u32 {
        self.first
    }
    pub fn end(&self) -> u32 {
        self.last
    }
    pub fn review_head(&self) -> &str {
        self.proof.read.parent().head_sha()
    }
    pub fn merge_base(&self) -> &str {
        self.proof.read.merge_base()
    }
    pub fn path(&self) -> &[u8] {
        self.proof.read.image(self.side).path()
    }
    pub fn scope(&self) -> &Scope {
        self.proof.source.scope()
    }
    pub fn original_pr_node(&self) -> &str {
        self.proof.read.parent().node_id()
    }
    pub fn original_repository_id(&self) -> u64 {
        self.proof.read.parent().repository_id()
    }
    pub fn original_pr_id(&self) -> u64 {
        self.proof.read.parent().database_id()
    }
    pub fn patch_digest(&self) -> &str {
        self.proof.read.patch_digest()
    }
}
pub struct PrImmutableImageResultV1 {
    page: PrPage,
    pub requested_navigation: Arc<ScopedPrNavigationV1>,
    pub source: Arc<CatalogueIntentSource>,
    pub active: Arc<ActiveRemoteScopeV1>,
    navigation: Arc<ScopedPrNavigationV1>,
    view: Arc<ScopedPrResourceViewV1>,
    cached_selected: PrKey,
    pub proof: Arc<ClosedPrFileImagesV1>,
    pub viewer: Option<ui::PrImageViewerV1>,
    pub display_limited: bool,
    pub closed_identity: crate::providers::gh::HostIdentity,
    pub accounting: crate::providers::runner::ScopedReadAccountingV1,
}
impl std::fmt::Debug for PrImmutableImageResultV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrImmutableImageResultV1(<original private image page>)")
    }
}
#[allow(clippy::too_many_arguments)]
pub fn read_images(
    source: Arc<CatalogueIntentSource>,
    active: Arc<ActiveRemoteScopeV1>,
    input: PrImmutableImageInputV1,
    provider: &GithubProvider,
    database: &Path,
    limits: &PrImmutableFileLimitsV1,
    access: &RepositoryCatalogueAccessLimits,
    cancel: &CancellationToken,
) -> Result<PrImmutableImageResultV1, PrResourceFailureV1> {
    read_images_with_guard(
        source,
        active,
        input,
        provider,
        database,
        limits,
        access,
        cancel,
        |_| Ok(()),
    )
}
#[allow(clippy::too_many_arguments)]
pub fn read_images_with_guard(
    source: Arc<CatalogueIntentSource>,
    active: Arc<ActiveRemoteScopeV1>,
    mut input: PrImmutableImageInputV1,
    provider: &GithubProvider,
    database: &Path,
    limits: &PrImmutableFileLimitsV1,
    access: &RepositoryCatalogueAccessLimits,
    cancel: &CancellationToken,
    mut hook: impl FnMut(PrResourcePhaseV1) -> Result<(), GithubError>,
) -> Result<PrImmutableImageResultV1, PrResourceFailureV1> {
    limits.validate().map_err(|e| refused(e.kind))?;
    let nav = &input.input.observed.navigation;
    let RemoteScopeConsumerV1::Explicit(rows) = active.consumer(&input.input.observed.identity)
    else {
        return Err(refused(GithubErrorKind::InvalidTarget));
    };
    if nav.scope != *source.scope()
        || !matches_page(nav, &input.input.observed.page)
        || !rows.references().iter().any(|row| {
            row.original() == &nav.original
                && row.observed().is_some_and(|observed| {
                    observed
                        .last_name
                        .eq_ignore_ascii_case(&input.input.observed.cached_selected.repository)
                })
        })
        || input.input.observed.page.tab != PrTab::Files
    {
        return Err(refused(GithubErrorKind::InvalidTarget));
    }
    let view = input
        .input
        .previous_view
        .as_ref()
        .ok_or_else(|| refused(GithubErrorKind::InvalidTarget))?
        .clone();
    let original = PrImmutableFileOriginalV1::from_observed(
        source.scope(),
        view.observed.clone(),
        input.file_index,
        limits,
    )
    .map_err(|e| refused(e.kind))?;
    if original.original_parent().node_id() != nav.node
        || original.original_parent().repository_id() != nav.original.id
        || original.original_parent().number() != nav.canonical.number
        || !original
            .original_parent()
            .repository()
            .eq_ignore_ascii_case(&nav.canonical.repository)
        || original.original_parent().head_sha() != nav.head
        || original.original_parent().base_tip_sha() != nav.base
    {
        return Err(refused(GithubErrorKind::InvalidTarget));
    }
    let mut session = provider
        .begin_repository_catalogue_access(source.source(), access, cancel)
        .map_err(|e| refused(e.kind))?;
    let identity = session.identity().clone();
    let window = session
        .reserve_measured_external_reads(
            original.max_external_requests(),
            1024 * 1024,
            limits.max_pool_bytes,
            2,
            cancel,
        )
        .map_err(|e| refused(e.kind))?;
    let bounded = provider
        .clone()
        .with_measured_read_window(window)
        .map_err(|e| refused(e.kind))?;
    let current = session
        .verify(&nav.original, cancel)
        .map_err(|e| refused(e.kind))?;
    let mut local = None;
    let observed = bounded
        .pr_immutable_file_images_v1(current, &identity, &original, limits, cancel, |parent| {
            local = Some(ResourceLocalGuard::capture(
                database,
                &source,
                &identity,
                parent.summary(),
            )?);
            hook(PrResourcePhaseV1::AfterOpeningSource).map_err(|e| GithubError {
                kind: e.kind,
                message: "Original image opening guard unavailable".into(),
            })
        })
        .map_err(|e| refused(e.kind))?;
    let canonical = observed.parent().repository().to_owned();
    let closing = session
        .verify(&nav.original, cancel)
        .map_err(|e| refused(e.kind))?;
    if closing.repository().full_name != canonical
        || closing.repository().permissions.pull != Some(true)
    {
        return Err(refused(GithubErrorKind::IdentityChanged));
    }
    let deadline = session.deadline();
    let closed = session
        .close_with_guard(cancel, || {
            hook(PrResourcePhaseV1::BeforeClosingAccount).map_err(|e| GithubError {
                kind: e.kind,
                message: "Original image closing guard unavailable".into(),
            })?;
            local
                .as_ref()
                .ok_or_else(safe_local)?
                .validate(database, &source)
        })
        .map_err(|e| refused(e.kind))?;
    let navigation = Arc::new(
        ScopedPrNavigationV1::bind(
            source.scope(),
            &nav.original,
            &PrKey {
                host: source.scope().host.clone(),
                repository: canonical.clone(),
                number: nav.canonical.number,
            },
            observed.parent().summary(),
        )
        .map_err(|_| refused(GithubErrorKind::InvalidTarget))?,
    );
    let requested_navigation = input.input.observed.navigation.clone();
    input.input.observed.page.details.summary.pr.repository = canonical;
    let old_lines = line_offsets(observed.image(PrImageSideV1::Old).bytes())
        .map_err(|_| refused(GithubErrorKind::Unsupported))?;
    let new_lines = line_offsets(observed.image(PrImageSideV1::New).bytes())
        .map_err(|_| refused(GithubErrorKind::Unsupported))?;
    let proof = Arc::new(ClosedPrFileImagesV1 {
        read: Arc::new(observed),
        original_view: view.clone(),
        source: source.clone(),
        active: active.clone(),
        navigation: navigation.clone(),
        old_lines,
        new_lines,
    });
    // Pure worker display preflight. Preserve the existing Files page and all
    // row/scroll/focus on refusal; do not label a display bound access denial.
    let viewer = ui::PrImageViewerV1::prepare(proof.clone()).ok();
    let display_limited = viewer.is_none();
    if cancel.is_cancelled() || std::time::Instant::now() >= deadline {
        return Err(refused(GithubErrorKind::Transport));
    }
    Ok(PrImmutableImageResultV1 {
        page: input.input.observed.page,
        requested_navigation,
        source,
        active,
        navigation,
        view,
        cached_selected: input.input.observed.cached_selected,
        proof,
        viewer,
        display_limited,
        closed_identity: closed.identity().clone(),
        accounting: *closed
            .external_read_accounting()
            .ok_or_else(|| refused(GithubErrorKind::Transport))?,
    })
}
fn line_offsets(bytes: Option<&[u8]>) -> Result<Vec<(usize, usize)>, ()> {
    let Some(bytes) = bytes else {
        return Ok(vec![]);
    };
    if bytes.split_inclusive(|byte| *byte == b'\n').count() > MAX_PREPARED_ROWS {
        return Err(());
    }
    let mut offset = 0;
    Ok(bytes
        .split_inclusive(|byte| *byte == b'\n')
        .map(|line| {
            let start = offset;
            offset += line.len();
            (start, offset)
        })
        .collect())
}
/// Existing Runtime checks SAME source/scope/nav+generation first, then this
/// exact slot installer before any identity/image/overlay/focus effect.
pub fn install_images(
    app: &mut crate::app::App,
    result: PrImmutableImageResultV1,
) -> Result<(Option<ui::PrImageViewerV1>, bool), ObservedPrTabFailureV1> {
    if !app
        .remote_scope_source
        .as_ref()
        .is_some_and(|source| Arc::ptr_eq(source, &result.source))
        || !app
            .remote_scope
            .as_ref()
            .is_some_and(|active| Arc::ptr_eq(active, &result.active))
    {
        return Err(ObservedPrTabFailureV1::SelectedPageUnavailable);
    }
    let github = app
        .github
        .as_mut()
        .ok_or(ObservedPrTabFailureV1::SelectedPageUnavailable)?;
    if github.pr.is_some()
        || github.scope().ok().as_ref() != Some(result.source.scope())
        || github
            .scoped_pr_navigation
            .as_ref()
            .is_none_or(|nav| !Arc::ptr_eq(nav, &result.requested_navigation))
        || github.selected.as_ref() != Some(&result.cached_selected)
        || !matches_page(&result.navigation, &result.page)
    {
        return Err(ObservedPrTabFailureV1::SelectedPageUnavailable);
    }
    github.identity = result.closed_identity;
    github.scoped_pr_navigation = Some(result.navigation);
    github.scoped_pr_resource = Some(result.view);
    github.pr = Some(result.page);
    Ok((result.viewer, result.display_limited))
}
