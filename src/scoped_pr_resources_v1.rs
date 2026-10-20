//! SOURCE-ONLY child of observed tabs: current selected Files/Checks under the
//! SAME genuine source/session. Page-only preparation, never a write permit.
use super::*;
use crate::{
    github_workspace::{PrPage, PrRow},
    pr_write_journal::{PrWriteJournal, PrWriteTarget},
    providers::gh::{
        GithubError, GithubErrorKind, GithubProvider, PrResourceKindV1, PrResourceReadLimitsV1,
        PrResourceReadV1, RepositoryCatalogueAccessLimits,
    },
    repository_view::DiffLayout,
    storage::Cache,
    theme::SemanticToken,
};
use std::{
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};
#[path = "scoped_pr_immutable_images_v1.rs"]
pub mod immutable_images;
#[path = "scoped_pr_resources_native_v1.rs"]
pub mod native;
#[path = "scoped_pr_threads_v1.rs"]
pub mod threads_v1;

pub const OLD_IMAGE_UNVERIFIED: &str =
    "Old side: current base-branch tip, which can differ from the merge base";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrResourcePhaseV1 {
    AfterOpeningSource,
    BeforeClosingAccount,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrResourceUiActionV1 {
    NativeDiff,
    Threads,
    NextFile,
    NextHunk,
    Layout,
    PatchContext,
    OldImage,
    /// Open the canonical images at the selected diff line, ready to comment.
    CommentLine,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrResourceRowV1 {
    Notice,
    FileList(usize),
    File(usize),
    Hunk(usize, usize),
    Line(usize, usize),
    Check(bool, usize),
}
pub struct PrResourceInputV1 {
    observed: ObservedPrTabInputV1,
    kind: PrResourceKindV1,
    previous_view: Option<Arc<ScopedPrResourceViewV1>>,
}
/// Owned source data exists only after actual original close_with_guard. These
/// getters are read navigation/evidence, NOT current permissions for later work.
pub struct ScopedPrResourceViewV1 {
    observed: Arc<PrResourceReadV1>,
    diff: Option<Arc<crate::diff::Diff>>,
    targets: Vec<PrResourceRowV1>,
    pub layout: DiffLayout,
    pub show_context: bool,
}
impl std::fmt::Debug for ScopedPrResourceViewV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedPrResourceViewV1(<closed observed read; not a write permit>)")
    }
}
impl ScopedPrResourceViewV1 {
    pub(crate) fn workflow_owned_bytes_v9(&self) -> Option<usize> {
        use crate::providers::gh::workflow_capacity_v9 as b;
        b::sum([
            b::owner::<Self>(),
            self.observed.workflow_owned_bytes_v9()?,
            b::vector(&self.targets)?,
            match &self.diff {
                Some(diff) => b::diff(diff)?,
                None => 0,
            },
        ])
    }
    pub fn observed(&self) -> &PrResourceReadV1 {
        &self.observed
    }
    pub fn canonical_diff(&self) -> Option<&crate::diff::Diff> {
        self.diff.as_deref()
    }
    pub fn row(&self, index: usize) -> Option<PrResourceRowV1> {
        self.targets.get(index).copied()
    }
    /// The changed file of a file-list row, a file header, a hunk or a diff line: its new path
    /// (the old one for a deletion). For File history (TASK-99.3); no other meaning.
    pub fn file_path_at(&self, index: usize) -> Option<crate::git::RawPath> {
        match self.row(index)? {
            PrResourceRowV1::FileList(i) => Some(crate::git::RawPath(
                self.observed.files().get(i)?.path.as_bytes().to_vec(),
            )),
            PrResourceRowV1::File(i)
            | PrResourceRowV1::Hunk(i, _)
            | PrResourceRowV1::Line(i, _) => {
                let file = self.diff.as_ref()?.files.get(i)?;
                file.new_path
                    .as_deref()
                    .or(file.old_path.as_deref())
                    .map(|path| crate::git::RawPath(path.to_vec()))
            }
            _ => None,
        }
    }
    pub fn check(&self, row: usize) -> Option<&crate::providers::gh::ObservedPrCheckV1> {
        match self.row(row)? {
            PrResourceRowV1::Check(false, i) => self.observed.checks().get(i),
            PrResourceRowV1::Check(true, i) => self.observed.statuses().get(i),
            _ => None,
        }
    }
    pub fn next(&self, row: usize, hunk: bool) -> Option<usize> {
        let admits = |target: &PrResourceRowV1| {
            if hunk {
                matches!(target, PrResourceRowV1::Hunk(_, _))
            } else {
                matches!(
                    target,
                    PrResourceRowV1::File(_) | PrResourceRowV1::FileList(_)
                )
            }
        };
        self.targets
            .iter()
            .enumerate()
            .skip(row.saturating_add(1))
            .find(|(_, target)| admits(target))
            .or_else(|| {
                self.targets
                    .iter()
                    .enumerate()
                    .find(|(_, target)| admits(target))
            })
            .map(|(i, _)| i)
    }
    /// Borrowed bounded LOADED PATCH context. No unobserved file lines or API
    /// calls are synthesized; old snapshot remains unavailable even on Old side.
    pub fn patch_context(&self, row: usize, radius: usize) -> Option<Vec<String>> {
        if radius > 50 {
            return None;
        }
        let (file, hunk) = match self.row(row)? {
            PrResourceRowV1::Hunk(f, h) | PrResourceRowV1::Line(f, h) => (f, h),
            _ => return None,
        };
        let hunk = self.diff.as_ref()?.files.get(file)?.hunks.get(hunk)?;
        let limit = radius.saturating_mul(2).saturating_add(1);
        let notice_bytes = hunk
            .lines
            .iter()
            .take(limit)
            .try_fold(256usize, |bytes, line| {
                bytes.checked_add(line.text.len().checked_mul(6)?.checked_add(128)?)
            });
        if notice_bytes.is_none_or(|bytes| bytes > 15_000) {
            return None;
        }
        Some(
            std::iter::once("Loaded patch context only; not the complete immutable file".into())
                .chain(hunk.lines.iter().take(limit).map(|line| {
                    format!(
                        "old {:?} new {:?}: {}",
                        line.old_line,
                        line.new_line,
                        String::from_utf8_lossy(&line.text)
                    )
                }))
                .collect(),
        )
    }
    /// Worker-only pure rebuild with shared immutable source/patch pointers.
    /// Display refusal never mutates the currently admitted rows/targets/layout.
    pub fn prepared_layout(
        &self,
        layout: DiffLayout,
        context: bool,
        cancel: &CancellationToken,
    ) -> Result<Option<(Self, Vec<PrRow>)>, String> {
        let mut next = Self {
            observed: self.observed.clone(),
            diff: self.diff.clone(),
            targets: vec![],
            layout,
            show_context: context,
        };
        Ok(next.prepare_rows(cancel)?.map(|rows| (next, rows)))
    }
}
pub struct PrResourceResultV1 {
    pub page: PrPage,
    pub requested_navigation: Arc<ScopedPrNavigationV1>,
    pub navigation: Arc<ScopedPrNavigationV1>,
    pub cached_selected: PrKey,
    pub original_source: Arc<CatalogueIntentSource>,
    pub original_active: Arc<ActiveRemoteScopeV1>,
    pub view: Option<Arc<ScopedPrResourceViewV1>>,
    pub closed_identity: crate::providers::gh::HostIdentity,
    pub accounting: crate::providers::runner::ScopedReadAccountingV1,
    pub display_limited: bool,
}
impl std::fmt::Debug for PrResourceResultV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrResourceResultV1")
            .field("display_limited", &self.display_limited)
            .finish_non_exhaustive()
    }
}
/// Runtime must first match SAME source/scope/nav owner plus operation generation.
/// The exact pending page slot then rejects Back/reselection before ANY effects.
pub fn install_resource_if_current(
    workspace: &mut GithubWorkspace,
    result: PrResourceResultV1,
) -> Result<(), ObservedPrTabFailureV1> {
    if workspace.scope().ok().as_ref() != Some(result.original_source.scope())
        || workspace.pr.is_some()
        || workspace
            .scoped_pr_navigation
            .as_ref()
            .is_none_or(|nav| !Arc::ptr_eq(nav, &result.requested_navigation))
        || workspace.selected.as_ref() != Some(&result.cached_selected)
        || !matches_page(&result.navigation, &result.page)
    {
        return Err(ObservedPrTabFailureV1::SelectedPageUnavailable);
    }
    workspace.identity = result.closed_identity;
    workspace.scoped_pr_navigation = Some(result.navigation);
    workspace.scoped_pr_resource = result.view;
    workspace.pr = Some(result.page);
    Ok(())
}
#[derive(Debug)]
pub struct PrResourceFailureV1(pub GithubErrorKind);
impl std::fmt::Display for PrResourceFailureV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Selected PR resource unavailable • previous private section hidden • no default scope or write")
    }
}
impl std::error::Error for PrResourceFailureV1 {}
fn refused(kind: GithubErrorKind) -> PrResourceFailureV1 {
    PrResourceFailureV1(kind)
}
pub(crate) fn safe_local() -> GithubError {
    GithubError {
        kind: GithubErrorKind::IdentityChanged,
        message: "Original selected resource local fence unavailable".into(),
    }
}
pub fn take_resource_input(
    workspace: &mut GithubWorkspace,
    kind: PrResourceKindV1,
) -> Result<PrResourceInputV1, ObservedPrTabFailureV1> {
    let observed = take_input(workspace, ObservedPrTabV1::Summary)?;
    Ok(PrResourceInputV1 {
        observed,
        kind,
        previous_view: workspace.scoped_pr_resource.take(),
    })
}
/// Whole set stays immutable. No first repo, manufactured Read or saved-set
/// adoption. Genuine current canonical name may change only this selected page.
#[allow(clippy::too_many_arguments)]
pub fn read_resource(
    original: Arc<CatalogueIntentSource>,
    active: Arc<ActiveRemoteScopeV1>,
    input: PrResourceInputV1,
    provider: &GithubProvider,
    database: &Path,
    limits: &PrResourceReadLimitsV1,
    access_limits: &RepositoryCatalogueAccessLimits,
    cancel: &CancellationToken,
) -> Result<PrResourceResultV1, PrResourceFailureV1> {
    read_resource_with_guard(
        original,
        active,
        input,
        provider,
        database,
        limits,
        access_limits,
        cancel,
        |_| Ok(()),
    )
}
#[allow(clippy::too_many_arguments)]
pub fn read_resource_with_guard(
    original: Arc<CatalogueIntentSource>,
    active: Arc<ActiveRemoteScopeV1>,
    mut input: PrResourceInputV1,
    provider: &GithubProvider,
    database: &Path,
    limits: &PrResourceReadLimitsV1,
    access_limits: &RepositoryCatalogueAccessLimits,
    cancel: &CancellationToken,
    mut hook: impl FnMut(PrResourcePhaseV1) -> Result<(), GithubError>,
) -> Result<PrResourceResultV1, PrResourceFailureV1> {
    limits.validate().map_err(|e| refused(e.kind))?;
    let observed = &input.observed;
    let nav = &observed.navigation;
    let RemoteScopeConsumerV1::Explicit(references) = active.consumer(&observed.identity) else {
        return Err(refused(GithubErrorKind::InvalidTarget));
    };
    if Scope::github(&observed.identity.host, observed.identity.account_id)
        .ok()
        .as_ref()
        != Some(original.scope())
        || &nav.scope != original.scope()
        || !matches_page(nav, &observed.page)
        || !references.references().iter().any(|row| {
            row.original() == &nav.original
                && row.observed().is_some_and(|value| {
                    value
                        .last_name
                        .eq_ignore_ascii_case(&observed.cached_selected.repository)
                })
        })
    {
        return Err(refused(GithubErrorKind::InvalidTarget));
    }
    let mut session = provider
        .begin_repository_catalogue_access(original.source(), access_limits, cancel)
        .map_err(|e| refused(e.kind))?;
    if Scope::github(&session.identity().host, session.identity().account_id)
        .ok()
        .as_ref()
        != Some(original.scope())
    {
        return Err(refused(GithubErrorKind::IdentityChanged));
    }
    let identity = session.identity().clone();
    let window = session
        .reserve_measured_external_reads(
            limits.max_external_requests(input.kind),
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
    let resource = bounded
        .pr_resource_read_v1(
            current,
            &identity,
            &observed.page.details.summary,
            input.kind,
            limits,
            cancel,
            |parent| {
                let guard =
                    ResourceLocalGuard::capture(database, &original, &identity, parent.summary())?;
                hook(PrResourcePhaseV1::AfterOpeningSource).map_err(|failure| GithubError {
                    kind: failure.kind,
                    message: "Selected resource hook unavailable".into(),
                })?;
                local = Some(guard);
                Ok(())
            },
        )
        .map_err(|e| refused(e.kind))?;
    let canonical = resource.parent().summary().pr.repository.clone();
    let closing = session
        .verify(&nav.original, cancel)
        .map_err(|e| refused(e.kind))?;
    if closing.repository().full_name != canonical
        || closing.repository().permissions.pull != Some(true)
    {
        return Err(refused(GithubErrorKind::IdentityChanged));
    }
    // Critical: original SQL recapture AFTER real closing repo, inside existing
    // close_with_guard, then final version/user2. Nothing private is read afterward.
    let deadline = session.deadline();
    let closed = session
        .close_with_guard(cancel, || {
            hook(PrResourcePhaseV1::BeforeClosingAccount).map_err(|failure| GithubError {
                kind: failure.kind,
                message: "Selected resource closing hook unavailable".into(),
            })?;
            local
                .as_ref()
                .ok_or_else(safe_local)?
                .validate(database, &original)
        })
        .map_err(|e| refused(e.kind))?;
    let accounting = *closed
        .external_read_accounting()
        .ok_or_else(|| refused(GithubErrorKind::Transport))?;
    let (view, rows) =
        prepare_resource(resource, cancel).map_err(|_| refused(GithubErrorKind::Json))?;
    let requested_navigation = input.observed.navigation.clone();
    let display_limited = rows.is_none();
    let navigation = if let Some(rows) = rows {
        input.observed.page.tab = match input.kind {
            PrResourceKindV1::Files => PrTab::Files,
            PrResourceKindV1::Checks => PrTab::Checks,
        };
        input.observed.page.rows = rows;
        input.observed.page.details.summary.pr.repository = canonical.clone();
        // Legacy mutable-coordinate/check Vec adapters are deliberately NOT
        // installed; this selected source uses its own typed observed sidecar.
        input.observed.page.diff = None;
        input.observed.page.checks = None;
        Arc::new(
            ScopedPrNavigationV1::bind(
                original.scope(),
                &requested_navigation.original,
                &PrKey {
                    host: original.scope().host.clone(),
                    repository: canonical,
                    number: input.observed.page.details.summary.pr.number,
                },
                &input.observed.page.details.summary,
            )
            .map_err(|_| refused(GithubErrorKind::InvalidTarget))?,
        )
    } else {
        requested_navigation.clone()
    };
    if cancel.is_cancelled() || std::time::Instant::now() >= deadline {
        return Err(refused(GithubErrorKind::Transport));
    }
    Ok(PrResourceResultV1 {
        page: input.observed.page,
        requested_navigation,
        navigation,
        cached_selected: input.observed.cached_selected,
        original_source: original,
        original_active: active,
        view: if display_limited {
            input.previous_view
        } else {
            view.map(Arc::new)
        },
        closed_identity: closed.identity().clone(),
        accounting,
        display_limited,
    })
}
/// Local original metadata only; normal Cache/journal open may initialize or
/// repair local policy/schema. No receipts/draft text read, claim, ACK or write.
pub(crate) struct ResourceLocalGuard {
    path: PathBuf,
    file: (u64, u64),
    target: PrWriteTarget,
    generation: i64,
}
/// Closed ORIGINAL boundary variants, not caller-supplied callbacks or flags.
#[derive(Clone, Copy)]
pub(crate) enum PrLocalSourceBoundaryV13<'a> {
    Catalogue(&'a CatalogueIntentSource),
    Personal(&'a crate::storage::PersonalPrSourceV13),
    PersonalReservation(&'a crate::storage::PersonalPrReservationV13),
    PersonalParent(&'a crate::storage::ClosedPersonalParentBoundaryV16),
    /// Only actual closed Notifications membership, not an arbitrary row/URL.
    NotificationsRead(&'a crate::providers::gh::ClosedNotificationReadV63),
    Notifications(&'a crate::providers::gh::NotificationPrSourceV63),
}
impl PrLocalSourceBoundaryV13<'_> {
    fn validate(self, cache: &Cache) -> Result<(), GithubError> {
        match self {
            Self::Catalogue(source) => cache
                .validate_repository_catalogue_intent_source(source)
                .map_err(|_| safe_local()),
            Self::Personal(source) => cache
                .validate_personal_pr_source_v13(source)
                .map_err(|_| safe_local()),
            Self::PersonalReservation(source) => cache
                .validate_personal_pr_reservation_v13(source)
                .map_err(|_| safe_local()),
            Self::PersonalParent(source) => cache
                .validate_personal_parent_boundary_v16(source)
                .map_err(|_| safe_local()),
            Self::NotificationsRead(source) => source.validate_allocation_v63(cache),
            Self::Notifications(source) => source.validate_cache(cache),
        }
    }
}
impl ResourceLocalGuard {
    /// State-only revocation guard. It borrows the already genuine opaque
    /// repair and cannot construct a source, writer guard or ACK permission.
    /// In particular the ordinary Pending/Confirmed validator is unchanged.
    pub(crate) fn validate_state_cache_repair_boundary_v75(
        &self,
        repair: &crate::pr_write_journal::state_origin_v68::StateCacheRepairV74,
    ) -> Result<(), GithubError> {
        let path = repair.database();
        if std::fs::canonicalize(path).map_err(|_| safe_local())? != self.path
            || file_identity(path)? != self.file
            || !repair.follows_original_guard_v75(&self.target, self.generation)
        {
            return Err(safe_local());
        }
        repair
            .source()
            .local_boundary()
            .validate(&Cache::open(path).map_err(|_| safe_local())?)?;
        repair.validate_current_raw_v75().map_err(|_| safe_local())
    }
    /// Closed State history only; no ordinary guard or writer bypass is issued.
    pub(crate) fn validate_state_history_v74(
        path: &Path,
        source: PrLocalSourceBoundaryV13<'_>,
        target: &PrWriteTarget,
        parent: &crate::providers::gh::PrResourceParentV1,
        history: &crate::pr_write_journal::state_origin_v68::history_v74::StateHistoryV74,
    ) -> Result<(), GithubError> {
        source.validate(&Cache::open(path).map_err(|_| safe_local())?)?;
        PrWriteJournal::open(path)
            .map_err(|_| safe_local())?
            .validate_state_history_v74(target, parent, history)
            .map_err(|_| safe_local())
    }
    /// A Metadata SECTION may display actual read-only facts around exactly
    /// this known Members blocker. This issues neither this ordinary guard nor
    /// a Source/current inspection/ACK permission and never admits another
    /// purpose through an ordinary guard error.
    pub(crate) fn validate_members_metadata_section_v62(
        path: &Path,
        source: PrLocalSourceBoundaryV13<'_>,
        target: &PrWriteTarget,
        parent: &crate::providers::gh::PrResourceParentV1,
        witness:&crate::pr_write_journal::metadata_members_v48::history_v48::MetadataMembersRecoveryWitnessV48,
    ) -> Result<(), GithubError> {
        if witness.origin.seed.scope != *target.scope()
            || !witness.origin.seed.same_object(parent)
            || target.node_id() != parent.node_id()
            || target.number() != parent.number()
            || !target
                .repository()
                .eq_ignore_ascii_case(parent.repository())
        {
            return Err(safe_local());
        }
        source.validate(&Cache::open(path).map_err(|_| safe_local())?)?;
        PrWriteJournal::open(path)
            .map_err(|_| safe_local())?
            .validate_metadata_members_inspection_v48(witness)
            .map_err(|_| safe_local())
    }
    /// Manual Issue history only: actual boundary + exact saved rows, never a
    /// writer guard, Current, selection or source from a protected record.
    pub(crate) fn validate_issue_link_history_v87(
        path: &Path,
        source: PrLocalSourceBoundaryV13<'_>,
        target: &PrWriteTarget,
        parent: &crate::providers::gh::PrResourceParentV1,
        history: &crate::pr_write_journal::issue_link_origin_v87::history_v87::IssueLinkHistoryV87,
    ) -> Result<(), GithubError> {
        source.validate(&Cache::open(path).map_err(|_| safe_local())?)?;
        PrWriteJournal::open(path)
            .map_err(|_| safe_local())?
            .validate_issue_link_history_v87(target, parent, history)
            .map_err(|_| safe_local())
    }
    /// Member history only. Exact blocker witnesses remain read-only and can
    /// never manufacture the ordinary ResourceLocalGuard needed by a writer.
    pub(crate) fn validate_metadata_members_history_v48(
        path: &Path,
        source: PrLocalSourceBoundaryV13<'_>,
        target: &PrWriteTarget,
        parent: &crate::providers::gh::PrResourceParentV1,
        history:&crate::pr_write_journal::metadata_members_v48::history_v48::MetadataMembersHistoryV48,
    ) -> Result<(), GithubError> {
        source.validate(&Cache::open(path).map_err(|_| safe_local())?)?;
        PrWriteJournal::open(path)
            .map_err(|_| safe_local())?
            .validate_metadata_members_history_v48(target, parent, history)
            .map_err(|_| safe_local())
    }
    /// Base-only read witness; no publisher guard or selection is issued.
    pub(crate) fn validate_base_history_v32(
        path: &Path,
        source: PrLocalSourceBoundaryV13<'_>,
        target: &PrWriteTarget,
        parent: &crate::providers::gh::PrResourceParentV1,
        history: &crate::pr_write_journal::metadata_base_origin_v32::history_v32::BaseHistoryV32,
    ) -> Result<(), GithubError> {
        source.validate(&Cache::open(path).map_err(|_| safe_local())?)?;
        PrWriteJournal::open(path)
            .map_err(|_| safe_local())?
            .validate_base_history_v32(target, parent, history)
            .map_err(|_| safe_local())
    }

    pub(crate) fn workflow_owned_bytes_v9(&self) -> Option<usize> {
        crate::providers::gh::workflow_capacity_v9::sum([
            std::mem::size_of::<Self>() + 2 * std::mem::size_of::<usize>(),
            self.path.capacity(),
            self.target.workflow_owned_bytes_v9()?,
        ])
    }
    /// Closed Metadata history only; no ordinary guard/source or write bypass.
    pub(crate) fn validate_metadata_history_v25(
        path: &Path,
        source: PrLocalSourceBoundaryV13<'_>,
        target: &PrWriteTarget,
        parent: &crate::providers::gh::PrResourceParentV1,
        history: &crate::pr_write_journal::metadata_origin_v25::history_v25::MetadataHistoryV25,
    ) -> Result<(), GithubError> {
        source.validate(&Cache::open(path).map_err(|_| safe_local())?)?;
        PrWriteJournal::open(path)
            .map_err(|_| safe_local())?
            .validate_metadata_history_v25(target, parent, history)
            .map_err(|_| safe_local())
    }
    /// Purpose-CLOSED protected Conversation history. This validates ONLY the
    /// actual source boundary and exact original known-purpose snapshot; it
    /// grants no ResourceLocalGuard or source to a writer with a blocker.
    pub(crate) fn validate_conversation_history_v17(
        path: &Path,
        source: PrLocalSourceBoundaryV13<'_>,
        target: &PrWriteTarget,
        parent: &crate::providers::gh::PrResourceParentV1,
        history: &crate::pr_write_journal::conversation_history_v17::ConversationHistoryV17,
    ) -> Result<(), GithubError> {
        source.validate(&Cache::open(path).map_err(|_| safe_local())?)?;
        PrWriteJournal::open(path)
            .map_err(|_| safe_local())?
            .validate_conversation_history_v17(target, parent, history)
            .map_err(|_| safe_local())
    }
    pub(crate) fn target_v17(&self) -> &PrWriteTarget {
        &self.target
    }
    pub(crate) fn generation_v17(&self) -> i64 {
        self.generation
    }
    /// Exact sole-journal owned step, not an epoch waiver. Ordinary reads keep
    /// their original generation/no-blocker guard. Only THIS actual marker may
    /// explain its own begin/claim/receipt generation during publication.
    pub(crate) fn validate_owned_boundary_v17(
        &self,
        path: &Path,
        source: PrLocalSourceBoundaryV13<'_>,
        marker: &crate::pr_write_journal::PrWriteMarker,
        generation: i64,
        state: crate::pr_write_journal::PrWriteState,
        receipt: Option<&crate::pr_write_journal::PrConfirmedEffect>,
    ) -> Result<(), GithubError> {
        use crate::pr_write_journal::PrWriteState;
        if generation <= self.generation
            || marker.target != self.target
            || std::fs::canonicalize(path).map_err(|_| safe_local())? != self.path
            || file_identity(path)? != self.file
        {
            return Err(safe_local());
        }
        source.validate(&Cache::open(path).map_err(|_| safe_local())?)?;
        let journal = PrWriteJournal::open(path).map_err(|_| safe_local())?;
        let actual = journal
            .original_publication(&self.target, &marker.publication_reference())
            .map_err(|_| safe_local())?
            .ok_or_else(safe_local)?;
        if actual.target != marker.target
            || actual.command != marker.command
            || actual.selected_resource != marker.selected_resource
            || actual.observed_head != marker.observed_head
            || actual.payload_sha256 != marker.payload_sha256
            || actual.created_at != marker.created_at
            || actual.state != state
            || actual.confirmed.as_ref() != receipt
            || journal
                .source_generation(&self.target)
                .map_err(|_| safe_local())?
                != generation
        {
            return Err(safe_local());
        }
        let blocking = journal
            .source_blocking(&self.target)
            .map_err(|_| safe_local())?;
        match state {
            PrWriteState::Pending
                if blocking.as_ref().is_some_and(|current| {
                    current.publication_reference() == marker.publication_reference()
                        && current.state == PrWriteState::Pending
                }) =>
            {
                Ok(())
            }
            PrWriteState::Confirmed if actual.confirmed.is_some() && blocking.is_none() => Ok(()),
            _ => Err(safe_local()),
        }
    }
    pub(crate) fn capture(
        path: &Path,
        original: &CatalogueIntentSource,
        identity: &crate::providers::gh::HostIdentity,
        summary: &PrSummary,
    ) -> Result<Self, GithubError> {
        Self::capture_boundary_v13(
            path,
            PrLocalSourceBoundaryV13::Catalogue(original),
            identity,
            summary,
        )
    }
    pub(crate) fn capture_boundary_v13(
        path: &Path,
        source: PrLocalSourceBoundaryV13<'_>,
        identity: &crate::providers::gh::HostIdentity,
        summary: &PrSummary,
    ) -> Result<Self, GithubError> {
        source.validate(&Cache::open(path).map_err(|_| safe_local())?)?;
        let journal = PrWriteJournal::open(path).map_err(|_| safe_local())?;
        let target = PrWriteTarget::new(identity, summary).map_err(|_| safe_local())?;
        let generation = journal
            .source_generation(&target)
            .map_err(|_| safe_local())?;
        if generation < 0
            || journal
                .source_blocking(&target)
                .map_err(|_| safe_local())?
                .is_some()
        {
            return Err(safe_local());
        }
        let path = journal.database_path().to_owned();
        let file = file_identity(&path)?;
        Ok(Self {
            path,
            file,
            target,
            generation,
        })
    }
    pub(crate) fn validate(
        &self,
        path: &Path,
        original: &CatalogueIntentSource,
    ) -> Result<(), GithubError> {
        self.validate_boundary_v13(path, PrLocalSourceBoundaryV13::Catalogue(original))
    }
    pub(crate) fn validate_boundary_v13(
        &self,
        path: &Path,
        source: PrLocalSourceBoundaryV13<'_>,
    ) -> Result<(), GithubError> {
        if std::fs::canonicalize(path).map_err(|_| safe_local())? != self.path
            || file_identity(path)? != self.file
        {
            return Err(safe_local());
        }
        source.validate(&Cache::open(path).map_err(|_| safe_local())?)?;
        let journal = PrWriteJournal::open(path).map_err(|_| safe_local())?;
        if journal
            .source_generation(&self.target)
            .map_err(|_| safe_local())?
            != self.generation
            || journal
                .source_blocking(&self.target)
                .map_err(|_| safe_local())?
                .is_some()
        {
            return Err(safe_local());
        }
        Ok(())
    }
}
fn file_identity(path: &Path) -> Result<(u64, u64), GithubError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| safe_local())?;
    // SAFETY: supported macOS/Linux effective uid lookup has no side effects.
    let owner = unsafe { libc::geteuid() };
    if !metadata.is_file()
        || metadata.uid() != owner
        || metadata.nlink() != 1
        || metadata.mode() & 0o077 != 0
    {
        return Err(safe_local());
    }
    Ok((metadata.dev(), metadata.ino()))
}
fn row(label: String, role: SemanticToken) -> PrRow {
    PrRow {
        label,
        role,
        actor: None,
        anchor: None,
        comment: None,
        cells: None,
    }
}
fn coverage(label: &str, value: &crate::providers::gh::PrResourceCoverageV1) -> String {
    let loaded = value.admitted_rows;
    match value.reported_total {
        _ if value.complete_observed() && loaded == 0 => format!("{label}: none"),
        Some(total) if loaded == 0 => format!("{label}: list not loaded ({total} total)"),
        _ if value.complete_observed() => format!("{label}: all {loaded} loaded"),
        Some(total) => format!("{label}: {loaded} of {total} loaded"),
        None => format!("{label}: {loaded} loaded (total unknown)"),
    }
}
fn prepare_resource(
    observed: PrResourceReadV1,
    cancel: &CancellationToken,
) -> Result<(Option<ScopedPrResourceViewV1>, Option<Vec<PrRow>>), String> {
    prepare_resource_arc_v13(Arc::new(observed), cancel)
}
/// SAME parser/row foundation, with the actual read Arc shared by the new
/// personal source rather than cloning private patch/source material.
pub(crate) fn prepare_resource_arc_v13(
    observed: Arc<PrResourceReadV1>,
    cancel: &CancellationToken,
) -> Result<(Option<ScopedPrResourceViewV1>, Option<Vec<PrRow>>), String> {
    if cancel.is_cancelled() {
        return Err("cancelled".into());
    }
    let mut view = ScopedPrResourceViewV1 {
        observed,
        diff: None,
        targets: vec![],
        layout: DiffLayout::Unified,
        show_context: true,
    };
    if view.observed.kind() == PrResourceKindV1::Files {
        if let Some(raw) = view.observed.raw_patch() {
            if raw.split(|byte| *byte == b'\n').count() > MAX_PREPARED_ROWS {
                return Ok((None, None));
            }
            validate_hunk_ranges(raw)?;
            let mut diff = crate::diff::Diff::parse(raw)?;
            diff.head = Some(view.observed.parent().summary().head_sha.clone());
            diff.base = None;
            view.diff = Some(Arc::new(diff));
        }
    }
    let Some(rows) = view.prepare_rows(cancel)? else {
        return Ok((None, None));
    };
    Ok((Some(view), Some(rows)))
}
impl ScopedPrResourceViewV1 {
    /// Borrowed checked preflight BEFORE row labels, actor clones or shared diff
    /// renderer copies. A refusal keeps the previously admitted section intact;
    /// it is a display bound, not incomplete source or denied repository access.
    fn within_display_bound(&self) -> bool {
        let mut rows = 5usize;
        let mut bytes = 1024usize;
        let mut add = |count: usize, text: usize| -> Option<()> {
            rows = rows.checked_add(count)?;
            bytes = bytes.checked_add(text)?;
            (rows <= MAX_PREPARED_ROWS && bytes <= MAX_PREPARED_TEXT).then_some(())
        };
        let estimated = (|| -> Option<()> {
            match self.observed.kind() {
                PrResourceKindV1::Files => {
                    for file in self.observed.files() {
                        add(
                            1,
                            file.path
                                .len()
                                .checked_add(file.status.as_ref().map_or(0, String::len))?
                                .checked_add(128)?,
                        )?;
                    }
                    if let Some(diff) = &self.diff {
                        for file in &diff.files {
                            let paths = file
                                .old_path
                                .as_ref()
                                .map_or(0, Vec::len)
                                .checked_add(file.new_path.as_ref().map_or(0, Vec::len))?;
                            // Display escaping may expand one ASCII control to
                            // six bytes. Keep both temporary split cells and the
                            // final combined label/anchor conservatively bounded.
                            add(1, paths.checked_mul(6)?.checked_add(128)?)?;
                            for metadata in &file.metadata {
                                add(1, metadata.len().checked_mul(6)?.checked_add(64)?)?;
                            }
                            for hunk in &file.hunks {
                                add(1, hunk.heading.len().checked_mul(6)?.checked_add(128)?)?;
                                for line in &hunk.lines {
                                    add(
                                        1,
                                        line.text
                                            .len()
                                            .checked_mul(12)?
                                            .checked_add(paths.checked_mul(6)?)?
                                            .checked_add(256)?,
                                    )?;
                                }
                            }
                        }
                    } else {
                        add(1, 128)?;
                    }
                }
                PrResourceKindV1::Checks => {
                    for value in self
                        .observed
                        .checks()
                        .iter()
                        .chain(self.observed.statuses())
                    {
                        let mut text = value.name.as_ref().map_or(0, String::len);
                        text = text.checked_add(value.status.as_ref().map_or(0, String::len))?;
                        text =
                            text.checked_add(value.conclusion.as_ref().map_or(0, String::len))?;
                        if let Some(actor) = &value.actor {
                            text = text
                                .checked_add(actor.login.len())?
                                .checked_add(actor.actor_type.len())?
                                .checked_add(actor.display_name.as_ref().map_or(0, String::len))?
                                .checked_add(actor.avatar_url.as_ref().map_or(0, String::len))?;
                        }
                        add(1, text.checked_add(256)?)?;
                    }
                }
            }
            Some(())
        })();
        estimated.is_some()
    }
    pub fn prepare_rows(
        &mut self,
        cancel: &CancellationToken,
    ) -> Result<Option<Vec<PrRow>>, String> {
        if cancel.is_cancelled() {
            return Err("cancelled".into());
        }
        if !self.within_display_bound() {
            return Ok(None);
        }
        let mut rows = vec![];
        self.targets.clear();
        let parent = self.observed.parent().summary();
        rows.push(row(
            format!(
                "Current observed head {} • base-branch tip {}",
                parent.head_sha, parent.base_sha
            ),
            SemanticToken::Muted,
        ));
        self.targets.push(PrResourceRowV1::Notice);
        match self.observed.kind() {
            PrResourceKindV1::Files => {
                rows.push(row(OLD_IMAGE_UNVERIFIED.into(), SemanticToken::Muted));
                self.targets.push(PrResourceRowV1::Notice);
                rows.push(row(
                    coverage("Files", self.observed.file_coverage()),
                    SemanticToken::Muted,
                ));
                self.targets.push(PrResourceRowV1::Notice);
                rows.push(row(
                    if self.observed.raw_patch().is_some() {
                        "Showing the PR patch from gh pr diff"
                    } else {
                        "Patch not loaded; file list only • open a file to view its lines"
                    }
                    .into(),
                    SemanticToken::Muted,
                ));
                self.targets.push(PrResourceRowV1::Notice);
                for (i, file) in self.observed.files().iter().enumerate() {
                    rows.push(row(
                        format!(
                            "File {} • {}{}",
                            file.path,
                            file.status.as_deref().unwrap_or("state unknown"),
                            if file.patch_supplied {
                                ""
                            } else {
                                " • API patch unavailable"
                            }
                        ),
                        SemanticToken::Reference,
                    ));
                    self.targets.push(PrResourceRowV1::FileList(i));
                }
                if let Some(diff) = &self.diff {
                    let start = self.targets.len();
                    for cell in
                        crate::repository_view::diff_rows(diff, self.layout, self.show_context)
                    {
                        let anchor = cell
                            .left
                            .anchor
                            .or_else(|| cell.right.as_ref().and_then(|right| right.anchor.clone()));
                        let target = anchor.as_ref().map_or(PrResourceRowV1::Notice, |anchor| {
                            PrResourceRowV1::Line(anchor.file_index, anchor.hunk_index)
                        });
                        let label = cell.right.map_or_else(
                            || cell.left.label.clone(),
                            |right| format!("{} │ {}", cell.left.label, right.label),
                        );
                        rows.push(PrRow {
                            label,
                            role: cell.left.role.into(),
                            actor: None,
                            anchor,
                            comment: None,
                            cells: None,
                        });
                        self.targets.push(target);
                    }
                    let mut cursor = start;
                    for (file_index, file) in diff.files.iter().enumerate() {
                        self.targets[cursor] = PrResourceRowV1::File(file_index);
                        cursor += 1 + crate::repository_view::diff_plain_metadata_len_v1(file);
                        for (hunk_index, hunk) in file.hunks.iter().enumerate() {
                            self.targets[cursor] = PrResourceRowV1::Hunk(file_index, hunk_index);
                            cursor += 1 + display_line_count(hunk, self.layout, self.show_context);
                        }
                    }
                    if cursor != self.targets.len() {
                        return Err("Typed patch row mapping differs from shared renderer".into());
                    }
                } else {
                    rows.push(row(
                        "Raw proposed patch unavailable; no empty or old-image result inferred"
                            .into(),
                        SemanticToken::Muted,
                    ));
                    self.targets.push(PrResourceRowV1::Notice);
                }
            }
            PrResourceKindV1::Checks => {
                for label in [
                    self.observed.required_rules_notice().into(),
                    self.observed.source_limitations().into(),
                    coverage("Check runs", self.observed.check_coverage()),
                    coverage("Status contexts", self.observed.status_coverage()),
                ] {
                    rows.push(row(label, SemanticToken::Muted));
                    self.targets.push(PrResourceRowV1::Notice);
                }
                for (status, values) in [
                    (false, self.observed.checks()),
                    (true, self.observed.statuses()),
                ] {
                    for (i, value) in values.iter().enumerate() {
                        let state = if value.current_context_unique {
                            value
                                .conclusion
                                .as_deref()
                                .or(value.status.as_deref())
                                .unwrap_or("unknown")
                        } else {
                            "unknown repeated context"
                        };
                        let role = match state {
                            "success" => SemanticToken::Success,
                            "failure" | "error" | "timed_out" | "action_required" => {
                                SemanticToken::Failure
                            }
                            "cancelled" => SemanticToken::Cancelled,
                            _ => SemanticToken::Pending,
                        };
                        let mut item = row(
                            format!(
                                "{}#{} • {} • {} • {}",
                                if status { "Status" } else { "Check" },
                                value.id,
                                value.name.as_deref().unwrap_or("name unknown"),
                                state,
                                if status {
                                    "not an Actions run"
                                } else {
                                    "observed head only"
                                }
                            ),
                            role,
                        );
                        item.actor = value.actor.clone();
                        if item.actor.is_none() {
                            item.label.push_str(" • actor unavailable");
                        }
                        rows.push(item);
                        self.targets.push(PrResourceRowV1::Check(status, i));
                    }
                }
            }
        }
        let bytes = rows
            .iter()
            .try_fold(0usize, |bytes, row| bytes.checked_add(row.label.len()));
        if rows.len() > MAX_PREPARED_ROWS || bytes.is_none_or(|bytes| bytes > MAX_PREPARED_TEXT) {
            return Ok(None);
        }
        if cancel.is_cancelled() {
            return Err("cancelled".into());
        }
        Ok(Some(rows))
    }
}
fn validate_hunk_ranges(raw: &[u8]) -> Result<(), String> {
    for line in raw
        .split(|byte| *byte == b'\n')
        .filter(|line| line.starts_with(b"@@ "))
    {
        let text = std::str::from_utf8(line).map_err(|_| "Hunk header not ASCII")?;
        let mut parts = text.split_whitespace();
        parts.next();
        for sign in ['-', '+'] {
            let part = parts
                .next()
                .and_then(|part| part.strip_prefix(sign))
                .ok_or("Invalid hunk header")?;
            let (start, count) = part.split_once(',').unwrap_or((part, "1"));
            let start = start.parse::<u32>().map_err(|_| "Invalid hunk start")?;
            let count = count.parse::<u32>().map_err(|_| "Invalid hunk count")?;
            start.checked_add(count).ok_or("Hunk coordinate overflow")?;
        }
    }
    Ok(())
}
fn display_line_count(hunk: &crate::diff::Hunk, layout: DiffLayout, context: bool) -> usize {
    use crate::diff::LineKind;
    let mut count = 0;
    let mut i = 0;
    while i < hunk.lines.len() {
        if !context && hunk.lines[i].kind == LineKind::Context {
            i += 1;
            continue;
        }
        if layout == DiffLayout::Split && hunk.lines[i].kind == LineKind::Removed {
            let start = i;
            while i < hunk.lines.len() && hunk.lines[i].kind == LineKind::Removed {
                i += 1;
            }
            let middle = i;
            while i < hunk.lines.len() && hunk.lines[i].kind == LineKind::Added {
                i += 1;
            }
            count += (middle - start).max(i - middle);
        } else {
            count += 1;
            i += 1;
        }
    }
    count
}
