//! Actual selected notification → fresh original PR membership. One moved
//! Core budget and one measured pool; no saved link/DTO becomes a PR source.
use super::*;
use crate::{
    repository_catalogue_intent_v1::IntentReference,
    scoped_pr_observed_tabs_v1::resources::{PrLocalSourceBoundaryV13, ResourceLocalGuard},
};
use std::path::{Path, PathBuf};

pub(crate) struct NotificationPrFollowIntentV63 {
    selected: NotificationSelectionV63,
    reference: IntentReference,
    number: u64,
}
impl std::fmt::Debug for NotificationPrFollowIntentV63 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NotificationPrFollowIntentV63(<genuine selected allocation; no access>)")
    }
}
impl NotificationSelectionV63 {
    pub(crate) fn pr_follow_intent_v63(&self) -> GithubResult<Arc<NotificationPrFollowIntentV63>> {
        let row = self.source.rows.get(self.index).ok_or_else(refused)?;
        let notifications_v63::NotificationSubjectV63::PullRequest { number } = row.subject else {
            return Err(error(
                GithubErrorKind::Unsupported,
                "This observed notification has no canonical PR target; no path or source guessed",
            ));
        };
        let reference = IntentReference {
            id: row.repository.id(),
            node_id: Some(row.repository.node().into()),
            last_name: row.repository.name().into(),
        };
        Ok(Arc::new(NotificationPrFollowIntentV63 {
            selected: NotificationSelectionV63 {
                source: self.source.clone(),
                index: self.index,
            },
            reference,
            number,
        }))
    }
}
impl NotificationPrFollowIntentV63 {
    pub(crate) fn host(&self) -> &str {
        &self.selected.source.identity().host
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        self.selected.source.identity()
    }
    pub(crate) fn reference(&self) -> &IntentReference {
        &self.reference
    }
    pub(crate) fn number(&self) -> u64 {
        self.number
    }
    pub(crate) fn belongs_to(&self, source: &Arc<ClosedNotificationReadV63>) -> bool {
        self.selected.belongs_to(source)
    }
}
/// Intermediate original phase, not publishable until preparation has
/// completed and finish consumes its exact thread/repo/SQL/final UID closure.
pub(crate) struct NotificationPrReadAccessV63<'a> {
    core: OriginalRepositoryAccessCoreV13<'a>,
    measured: GithubProvider,
    intent: Arc<NotificationPrFollowIntentV63>,
    original: Arc<NotificationRepositoryOriginalV63>,
    read: Arc<PrResourceReadV1>,
    local: ResourceLocalGuard,
    database: PathBuf,
    limits: PrResourceReadLimitsV1,
    token: CancellationToken,
    deadline: Instant,
}
/// Genuine closed source. It retains the actual issuer/allocation and local
/// journal/query fence, but every later action still starts a FRESH phase.
pub(crate) struct NotificationPrSourceV63 {
    intent: Arc<NotificationPrFollowIntentV63>,
    original: Arc<NotificationRepositoryOriginalV63>,
    read: Arc<PrResourceReadV1>,
    closed: ClosedOriginalRepositoryAccessV13,
    local: ResourceLocalGuard,
    database: PathBuf,
}
impl std::fmt::Debug for NotificationPrSourceV63 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NotificationPrSourceV63(<actual closed original PR; private source redacted>)")
    }
}
impl NotificationPrSourceV63 {
    pub(crate) fn belongs_to_notifications_v63(
        &self,
        source: &Arc<ClosedNotificationReadV63>,
    ) -> bool {
        self.intent.belongs_to(source)
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        &self.closed.identity
    }
    pub(crate) fn scope(&self) -> &crate::storage::Scope {
        self.intent.selected.source.scope()
    }
    pub(crate) fn reference(&self) -> &IntentReference {
        &self.intent.reference
    }
    pub(crate) fn read(&self) -> &Arc<PrResourceReadV1> {
        &self.read
    }
    pub(in crate::providers::gh::repository_catalogue) fn original(
        &self,
    ) -> &Arc<NotificationRepositoryOriginalV63> {
        &self.original
    }
    pub(crate) fn validate_cache(&self, cache: &Cache) -> GithubResult<()> {
        self.intent.selected.source.validate_allocation_v63(cache)?;
        self.local.validate_boundary_v13(
            &self.database,
            PrLocalSourceBoundaryV13::NotificationsRead(&self.intent.selected.source),
        )
    }
    #[cfg(test)]
    pub(crate) fn charged_bytes(&self) -> usize {
        self.closed.accounting.charged_bytes()
            + self
                .closed
                .measured_external_accounting
                .as_ref()
                .map_or(0, |a| a.charged_bytes())
    }
}
fn thread(
    provider: &GithubProvider,
    intent: &NotificationPrFollowIntentV63,
    token: &CancellationToken,
    deadline: Instant,
) -> GithubResult<NotificationRowV63> {
    // Only narrow THIS child; SAME measured Arc/runner/token/deadline remain.
    let child = provider
        .clone()
        .with_original_pr_read_bounds_v13(deadline, THREAD_CAP)?;
    let raw = child.api(
        &format!("notifications/threads/{}", intent.selected.id()),
        "GET",
        None,
        None,
        false,
        token,
    )?;
    let fresh = notifications_v63::parse::row(&provider.host, &raw)?;
    let old = &intent.selected.source.rows[intent.selected.index];
    if !old.same_original_target_v63(&fresh) || old.updated_at() != fresh.updated_at() {
        return Err(refused());
    }
    Ok(fresh)
}
fn read_retained(read: &PrResourceReadV1) -> Option<usize> {
    let mut bytes = std::mem::size_of::<PrResourceReadV1>()
        .checked_add(read.parent().retained_heap_bytes_v2())?
        .checked_add(read.files_capacity_bytes_v63())?
        .checked_add(read.raw_patch_capacity_bytes_v63())?;
    for file in read.files() {
        bytes = bytes
            .checked_add(file.path.capacity())?
            .checked_add(file.previous_path.as_ref().map_or(0, String::capacity))?
            .checked_add(file.status.as_ref().map_or(0, String::capacity))?
            .checked_add(file.blob_sha.as_ref().map_or(0, String::capacity))?;
    }
    Some(bytes)
}
fn display_peak(read: &PrResourceReadV1) -> Option<usize> {
    let mut bytes = read
        .parent()
        .retained_heap_bytes_v2()
        .checked_mul(4)?
        .checked_add(8192)?;
    for file in read.files() {
        bytes = bytes
            .checked_add(file.path.len().checked_mul(12)?)?
            .checked_add(
                file.status
                    .as_ref()
                    .map_or(0, String::len)
                    .checked_mul(12)?,
            )?
            .checked_add(1024)?;
    }
    if let Some(patch) = read.raw_patch() {
        let mut lines = 0usize;
        let mut path_bound = 0usize;
        for line in patch.split(|byte| *byte == b'\n') {
            lines = lines.checked_add(1)?;
            if [
                b"diff --git ".as_slice(),
                b"--- ",
                b"+++ ",
                b"rename from ",
                b"rename to ",
                b"copy from ",
                b"copy to ",
            ]
            .iter()
            .any(|prefix| line.starts_with(prefix))
            {
                // Decoding Git quoted paths cannot grow beyond their actual
                // header bytes. A lying extra patch member is NOT silently
                // assumed to have one of the API metadata's smaller paths.
                path_bound = path_bound.max(line.len());
            }
        }
        // Conservative peak BEFORE Diff/escaped row preparation: raw text and
        // escaped transient+retained labels (24x), repeated old/new paths (12x)
        // and parser/row/vector bookkeeping (1KiB per raw line). Existing row
        // and prepared-text bounds still apply independently; none is widened.
        bytes = bytes
            .checked_add(patch.len().checked_mul(24)?)?
            .checked_add(lines.checked_mul(path_bound.checked_mul(12)?.checked_add(1024)?)?)?;
    }
    Some(bytes)
}
impl GithubProvider {
    /// Caller must pass the original prequeue window and actual selected
    /// allocation. No transport getter, second pool or third repo GET exists.
    pub(crate) fn begin_notification_pr_follow_v63<'a>(
        &'a self,
        intent: Arc<NotificationPrFollowIntentV63>,
        cache: &Cache,
        database: &Path,
        limits: &PrResourceReadLimitsV1,
        token: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<NotificationPrReadAccessV63<'a>> {
        live(token, deadline)?;
        limits.validate()?;
        if self.scoped_read_budget.is_some()
            || !intent.selected.source.original.matches_issuer(self)
            || self
                .scoped_read_window
                .is_none_or(|(window, cap)| window > deadline || cap > 1024 * 1024)
            || deadline.saturating_duration_since(Instant::now()) > Duration::from_secs(30)
            || limits.max_pages_per_source > 4
            || limits.max_pool_bytes > WHOLE
            || database.as_os_str().len() > 4096
        {
            return Err(refused());
        }
        let deadline = deadline.min(self.scoped_read_window.expect("checked window").0);
        let requests = limits
            .max_pages_per_source
            .checked_add(5)
            .ok_or_else(refused)?;
        let pool = limits
            .max_pages_per_source
            .checked_add(2)
            .and_then(|n| n.checked_mul(limits.max_document_bytes))
            .and_then(|n| n.checked_add(THREAD_CAP * 2))
            .and_then(|n| n.checked_add(limits.max_patch_bytes))
            .ok_or_else(refused)?;
        if pool > limits.max_pool_bytes {
            return Err(refused());
        }
        let mut access = access_limits();
        access.max_references = 1;
        access.max_repository_reads = 2;
        let mut budget = CatalogueBudget::new(&access.transport)?;
        budget.original_cancellation = Some(token.clone());
        let identity = principal(self, &mut budget, token).map_err(safe_failure)?;
        same_principal(intent.identity(), &identity)?;
        intent
            .selected
            .source
            .validate_local(cache, token, deadline)?;
        let old = &intent.selected.source.rows[intent.selected.index];
        let original = NotificationRepositoryOriginalV63::from_observed(
            self,
            &identity,
            intent.selected.source.query(),
            std::slice::from_ref(old),
            true,
            METADATA,
        )?;
        let mut core = OriginalRepositoryAccessCoreV13::from_started_notifications_v63(
            self,
            original.clone(),
            identity,
            &access,
            budget,
            token,
        )?;
        if original.reference(0).as_ref() != Some(&intent.reference)
            || core
                .verify(&intent.reference, token)?
                .repository()
                .permissions
                .pull
                != Some(true)
        {
            return Err(refused());
        }
        // Exact P+5: thread/rawPR + FilesP + ONE raw patch + closing rawPR/thread.
        // Outer Core keeps opening+closing repo and first+final principal.
        let window = core.reserve_measured_external_reads(
            requests,
            limits
                .max_document_bytes
                .max(THREAD_CAP)
                .max(limits.max_patch_bytes),
            pool,
            1,
            token,
        )?;
        let measured = self.clone().with_measured_read_window(window)?;
        thread(&measured, &intent, token, deadline)?;
        let mut local = None;
        let current = core
            .observation_v1(intent.reference.id)
            .ok_or_else(refused)?;
        let read = measured.notification_pr_files_v63(
            current,
            core.identity(),
            &intent,
            limits,
            token,
            |parent| {
                local = Some(ResourceLocalGuard::capture_boundary_v13(
                    database,
                    PrLocalSourceBoundaryV13::NotificationsRead(&intent.selected.source),
                    core.identity(),
                    parent.summary(),
                )?);
                Ok(())
            },
        )?;
        live(token, deadline)?;
        let retained = intent
            .selected
            .source
            .retained_bytes_v63()
            .checked_add(original.retained_bytes())
            .and_then(|n| n.checked_add(core.metadata_bytes))
            .and_then(|n| n.checked_add(read_retained(&read)?))
            // Bound future escaped rows/doc/layout metadata BEFORE the caller
            // prepares. This is retained/peak accounting, not transport only.
            .and_then(|n| n.checked_add(display_peak(&read)?))
            .ok_or_else(refused)?;
        if retained > METADATA {
            return Err(refused());
        }
        Ok(NotificationPrReadAccessV63 {
            core,
            measured,
            intent,
            original,
            read: Arc::new(read),
            local: local.ok_or_else(refused)?,
            database: database.to_owned(),
            limits: limits.clone(),
            token: token.clone(),
            deadline,
        })
    }
}
const THREAD_CAP: usize = 128 * 1024;
impl NotificationPrReadAccessV63<'_> {
    pub(crate) fn read(&self) -> &Arc<PrResourceReadV1> {
        &self.read
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        self.core.identity()
    }
    pub(crate) fn reference(&self) -> &IntentReference {
        &self.intent.reference
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }
    pub(crate) fn finish(
        mut self,
        cache: &Cache,
        token: &CancellationToken,
    ) -> GithubResult<Arc<NotificationPrSourceV63>> {
        if !self.token.same_instance(token) {
            return Err(refused());
        }
        live(token, self.deadline)?;
        self.measured.notification_pr_closing_parent_v63(
            self.core
                .observation_v1(self.intent.reference.id)
                .ok_or_else(refused)?,
            &self.intent,
            &self.read,
            &self.limits,
            token,
        )?;
        thread(&self.measured, &self.intent, token, self.deadline)?;
        self.core.verify(&self.intent.reference, token)?;
        let closed = self.core.close_with_guard(token, || {
            self.intent
                .selected
                .source
                .validate_local(cache, token, self.deadline)?;
            self.local.validate_boundary_v13(
                &self.database,
                PrLocalSourceBoundaryV13::NotificationsRead(&self.intent.selected.source),
            )
        })?;
        live(token, self.deadline)?;
        if closed.identity.account_id != self.intent.identity().account_id
            || closed.verified.len() != 1
            || closed.verified[0].repository().id != self.read.parent().repository_id()
            || closed.verified[0].repository().node_id.as_deref()
                != Some(self.read.parent().repository_node())
        {
            return Err(refused());
        }
        Ok(Arc::new(NotificationPrSourceV63 {
            intent: self.intent,
            original: self.original,
            read: self.read,
            closed,
            local: self.local,
            database: self.database,
        }))
    }
}
#[cfg(test)]
#[path = "pr_follow_tests.rs"]
mod tests;
