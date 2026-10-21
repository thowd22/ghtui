//! Notifications on the SAME original Core/budget. No Catalogue/Personal/PR
//! source, extra authentication client, child pool or caller guard callback.
use super::notifications_http_v63::{page as http_page, NotificationHttpPageV63};
use super::original_repository_access_core_v13::ClosedOriginalRepositoryAccessV13;
use super::*;
use crate::storage::{Cache, NotificationSourceWitnessV63};
use notifications_v63::poll::{NotificationPollMetadataV63, NotificationValidatorsV63};
use notifications_v63::{
    NotificationAccumulatorV63, NotificationPageV63, NotificationQueryV63,
    NotificationReadContextV63, NotificationRepositoryOriginalV63, NotificationRowV63,
};

const WHOLE: usize = 8 * 1024 * 1024;
const METADATA: usize = 4 * 1024 * 1024;
const PAGE: usize = 256 * 1024;
const REPO: usize = 128 * 1024;
const REPOSITORIES: usize = 16;

#[path = "notification_access_v63/pr_follow.rs"]
mod pr_follow;
#[path = "notification_access_v63/publication.rs"]
mod publication;
pub(crate) use pr_follow::{NotificationPrFollowIntentV63, NotificationPrSourceV63};
pub(crate) use publication::{
    NotificationActionPreviewV63, NotificationInspectionV63, NotificationPublicationStateV63,
    NotificationPublicationV63, NotificationWriteReceiptV63,
};

/// The latest commenter of one selected notification, or why it is unknown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NotificationLatestActorV63 {
    /// `is_subject`: the latest activity URL was the subject itself, so the
    /// user is the subject's author rather than a commenter.
    Observed {
        actor: crate::providers::gh::Actor,
        is_subject: bool,
    },
    Unavailable(&'static str),
}
/// The list's latest actors, one per row in row order, from one batched read.
pub(crate) struct NotificationActorsV63 {
    pub(crate) actors: Vec<Option<crate::providers::gh::Actor>>,
    pub(crate) status: NotificationActorsStatusV63,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NotificationActorsStatusV63 {
    /// Every eligible row was read.
    Read,
    /// Only the first N rows were read; later rows show no actor.
    FirstRows(usize),
    /// No row names an issue or pull request with activity; nothing read.
    NoneEligible,
    /// The batched read failed or its window ended; every actor is unknown.
    Unavailable,
}
#[derive(Clone, Default)]
pub(crate) struct NotificationCoverageV63 {
    pub(crate) pages: usize,
    pub(crate) raw_rows: usize,
    pub(crate) duplicates: usize,
    pub(crate) unverified_repository_rows: usize,
    pub(crate) complete: bool,
    /// Pages the server answered `304 Not Modified` to our conditional GET.
    /// Equal to `pages` means the whole query was unchanged since the last read.
    pub(crate) not_modified: usize,
}
impl NotificationCoverageV63 {
    /// Every page of this read was a `304 Not Modified` answer.
    pub(crate) fn unchanged(&self) -> bool {
        self.pages > 0 && self.not_modified == self.pages
    }
}
struct NotificationCachedPageV63 {
    rows: Arc<Vec<NotificationRowV63>>,
    validators: NotificationValidatorsV63,
    exhausted: bool,
    retained: usize,
}
pub(crate) struct NotificationReadAccessV63<'a> {
    core: OriginalRepositoryAccessCoreV13<'a>,
    original: Arc<NotificationRepositoryOriginalV63>,
    witness: NotificationSourceWitnessV63,
    rows: Vec<NotificationRowV63>,
    pages: Vec<Arc<NotificationCachedPageV63>>,
    poll: NotificationPollMetadataV63,
    coverage: NotificationCoverageV63,
    original_cancel: CancellationToken,
    producing_runner: crate::providers::runner::CommandRunner,
    deadline: Instant,
}
/// Only a genuine session close constructs this. Allocation provenance is
/// lasting; each later explicit read/write still requires a fresh phase.
pub(crate) struct ClosedNotificationReadV63 {
    original: Arc<NotificationRepositoryOriginalV63>,
    witness: NotificationSourceWitnessV63,
    rows: Vec<NotificationRowV63>,
    pages: Vec<Arc<NotificationCachedPageV63>>,
    poll: NotificationPollMetadataV63,
    coverage: NotificationCoverageV63,
    closed: ClosedOriginalRepositoryAccessV13,
    repository_metadata_bytes: usize,
}
/// Actual selected allocation, never a label/index restored as authority. Each
/// use still requires the fresh original phase and exact current target.
pub(crate) struct NotificationSelectionV63 {
    source: Arc<ClosedNotificationReadV63>,
    index: usize,
}
impl NotificationSelectionV63 {
    pub(crate) fn id(&self) -> &str {
        self.source.rows[self.index].id()
    }
    pub(crate) fn belongs_to(&self, source: &Arc<ClosedNotificationReadV63>) -> bool {
        Arc::ptr_eq(&self.source, source)
    }
}
impl std::fmt::Debug for ClosedNotificationReadV63 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ClosedNotificationReadV63(<genuine original read; private rows hidden>)")
    }
}
impl ClosedNotificationReadV63 {
    pub(crate) fn validate_allocation_v63(&self, cache: &Cache) -> GithubResult<()> {
        cache
            .validate_notification_source_allocation_v63(&self.witness)
            .map_err(|_| refused())
    }
    pub(super) fn retained_bytes_v63(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.original.retained_bytes()
            + self.repository_metadata_bytes
            + self.rows.capacity() * std::mem::size_of::<NotificationRowV63>()
            + self
                .rows
                .iter()
                .map(NotificationRowV63::retained_bytes)
                .sum::<usize>()
            + self.pages.capacity() * std::mem::size_of::<Arc<NotificationCachedPageV63>>()
            + self.pages.iter().map(|page| page.retained).sum::<usize>()
    }
    pub(crate) fn select_row_v63(
        self: &Arc<Self>,
        index: usize,
    ) -> Option<NotificationSelectionV63> {
        self.rows.get(index).map(|_| NotificationSelectionV63 {
            source: self.clone(),
            index,
        })
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        &self.closed.identity
    }
    pub(crate) fn query(&self) -> &NotificationQueryV63 {
        self.original.query()
    }
    pub(crate) fn rows(&self) -> &[NotificationRowV63] {
        &self.rows
    }
    #[cfg(test)]
    pub(crate) fn coverage(&self) -> &NotificationCoverageV63 {
        &self.coverage
    }
    /// Starts a finite watch whose first poll waits for the LONGER of the
    /// configured refresh interval (at least 15 seconds) and this read's
    /// server `X-Poll-Interval`/`Retry-After`.
    pub(crate) fn begin_watch_v63(
        &self,
        configured_seconds: u64,
        now: Instant,
    ) -> GithubResult<notifications_v63::NotificationWatchV63> {
        if !(1..=4).contains(&self.coverage.pages) {
            return Err(refused());
        }
        let mut watch =
            notifications_v63::NotificationWatchV63::new(configured_seconds.clamp(15, 86400))?;
        if !watch.claim_due(now) {
            return Err(refused());
        }
        self.complete_watch_v63(&mut watch, now)?;
        Ok(watch)
    }
    pub(crate) fn complete_watch_v63(
        &self,
        watch: &mut notifications_v63::NotificationWatchV63,
        now: Instant,
    ) -> GithubResult<()> {
        watch.complete(now, &self.poll, self.witness.generation())
    }
    #[cfg(test)]
    pub(crate) fn generation(&self) -> u64 {
        self.witness.generation()
    }
    pub(crate) fn scope(&self) -> &crate::storage::Scope {
        self.witness.scope()
    }
    pub(crate) fn validate_local(
        &self,
        cache: &Cache,
        token: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        cache
            .validate_notification_source_v63(&self.witness, token, deadline)
            .map_err(|_| refused())
    }
    #[cfg(test)]
    pub(crate) fn accounting(&self) -> &RepositoryCatalogueAccounting {
        &self.closed.accounting
    }
}
fn refused() -> GithubError {
    error(
        GithubErrorKind::IdentityChanged,
        "Original notification source/account/query/database changed; private rows hidden",
    )
}
fn live(token: &CancellationToken, deadline: Instant) -> GithubResult<()> {
    if token.is_cancelled() || Instant::now() >= deadline {
        return Err(error(
            GithubErrorKind::Transport,
            "Original notification phase cancelled/expired; no clock renewed",
        ));
    }
    Ok(())
}
fn access_limits() -> RepositoryCatalogueAccessLimits {
    RepositoryCatalogueAccessLimits {
        transport: RepositoryCatalogueLimits {
            max_pages: 4,
            page_size: 50,
            max_rows: 200,
            max_document_bytes: REPO,
            max_metadata_bytes: METADATA,
            max_response_bytes: WHOLE,
            max_duration: Duration::from_secs(30),
        },
        max_references: REPOSITORIES,
        max_repository_reads: REPOSITORIES * 2,
    }
}
fn actual_page(
    provider: &GithubProvider,
    budget: &mut CatalogueBudget,
    query: &NotificationQueryV63,
    page: usize,
    validators: &NotificationValidatorsV63,
    token: &CancellationToken,
    deadline: Instant,
) -> GithubResult<crate::providers::runner::NotificationConditionalOutputV63> {
    live(token, deadline)?;
    budget.check(token)?;
    let cap = budget
        .remaining_bytes()
        .saturating_sub(REPOSITORIES * 2 * REPO + FINAL_PRINCIPAL_CAP)
        .min(PAGE);
    if cap == 0 {
        return Err(error(
            GithubErrorKind::Transport,
            "Notification page cannot consume closing repository/principal allowance",
        ));
    }
    let request = notifications_v63::NotificationConditionalRequestV63::page(
        provider, query, page, validators, token, deadline, cap,
    )?;
    let cap = request
        .runner_spec_v63(&provider.runner, token)?
        .output_limit;
    budget.accounting.requests += 1;
    let actual = match provider
        .runner
        .run_notification_conditional_get_v63(&request, token)
    {
        Ok(o) => o,
        Err(e) => {
            budget.accounting.failed_reserved_bytes = budget
                .accounting
                .failed_reserved_bytes
                .checked_add(cap)
                .ok_or_else(refused)?;
            return Err(e.into());
        }
    };
    let captured = actual.capture_v63();
    budget.accounting.stdout_bytes = budget
        .accounting
        .stdout_bytes
        .checked_add(captured.stdout_bytes())
        .ok_or_else(refused)?;
    budget.accounting.diagnostic_bytes = budget
        .accounting
        .diagnostic_bytes
        .checked_add(captured.stderr_bytes())
        .ok_or_else(refused)?;
    if captured.stdout_bytes() != actual.stdout_v63().len()
        || captured.total_bytes() > cap
        || budget.accounting.charged_bytes() > WHOLE
    {
        return Err(refused());
    }
    live(token, deadline)?;
    budget.check(token)?;
    Ok(actual)
}
impl GithubProvider {
    /// Bounds only: an already narrower issuer capture/deadline is never
    /// widened. The caller supplies the real prequeue phase deadline.
    pub(crate) fn with_notification_read_window_v63(&self, deadline: Instant) -> Self {
        let mut bounded = self.clone();
        bounded.scoped_read_window = Some(match self.scoped_read_window {
            Some((original, cap)) => (original.min(deadline), cap.min(PAGE)),
            None => (deadline, PAGE),
        });
        bounded
    }
    /// `deadline` is captured BEFORE queue admission. This factory refuses an
    /// unbounded or later-window client; it cannot manufacture a fresh clock.
    pub(crate) fn begin_notification_read_v63<'a>(
        &'a self,
        query: &NotificationQueryV63,
        expected: &HostIdentity,
        cache: &mut Cache,
        previous: Option<&Arc<ClosedNotificationReadV63>>,
        token: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<NotificationReadAccessV63<'a>> {
        live(token, deadline)?;
        query.validate()?;
        if deadline.saturating_duration_since(Instant::now()) > Duration::from_secs(30) {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Notification original queue-inclusive phase is at most30 seconds",
            ));
        }
        if self.scoped_read_budget.is_some()
            || self
                .scoped_read_window
                .is_none_or(|(bound, cap)| bound > deadline || cap > 1024 * 1024)
        {
            return Err(error(GithubErrorKind::InvalidTarget,"Notification factory requires its prequeued original window and unallocated same Core"));
        }
        let deadline = deadline.min(self.scoped_read_window.unwrap().0);
        if previous.is_some_and(|old| !old.original.matches_issuer(self) || old.query() != query) {
            return Err(refused());
        }
        let mut limits = access_limits();
        limits.validate()?;
        let mut budget = CatalogueBudget::new(&limits.transport)?;
        budget.original_cancellation = Some(token.clone());
        let identity = principal(self, &mut budget, token).map_err(safe_failure)?;
        live(token, deadline)?;
        // Foreground identity is INTENT, never access. Actual current numeric
        // principal must match before even reserving/reading private cache.
        same_principal(expected, &identity)?;
        if let Some(previous) = previous {
            same_principal(previous.identity(), &identity)?;
            previous.validate_local(cache, token, deadline)?;
        }
        let context = NotificationReadContextV63::from_actual(self, &identity, query)?;
        let witness = cache
            .reserve_notification_source_v63(
                &context,
                previous.map(|old| &old.witness),
                token,
                deadline,
            )
            .map_err(|_| refused())?;
        drop(context);
        let mut accumulated = NotificationAccumulatorV63::new();
        let mut pages = Vec::with_capacity(4);
        let mut cached_bytes = 0usize;
        let mut exhausted = false;
        let mut not_modified = 0usize;
        let mut last_poll = NotificationPollMetadataV63 {
            validators: NotificationValidatorsV63::default(),
            interval_seconds: None,
            retry_seconds: 0,
        };
        for ordinal in 1..=4 {
            let old = previous.and_then(|p| p.pages.get(ordinal - 1));
            let empty = NotificationValidatorsV63::default();
            let validators = old.map_or(&empty, |p| &p.validators);
            let actual = actual_page(
                self,
                &mut budget,
                query,
                ordinal,
                validators,
                token,
                deadline,
            )?;
            let (page, poll) = match http_page(actual.stdout_v63(), actual.exit_v63())? {
                NotificationHttpPageV63::Fresh { body, poll } => {
                    let parsed = notifications_v63::parse::page(
                        &self.host,
                        body.as_bytes(),
                        METADATA.saturating_sub(cached_bytes),
                    )?;
                    let metadata = notifications_v63::poll::metadata(&poll, utc_now()?)?;
                    let complete = if ordinal < 4 {
                        let next = notifications_v63::poll::next_page(
                            &self.host, query, ordinal, poll.link,
                        )?;
                        parsed.exhausted && next.is_none()
                    } else {
                        parsed.exhausted && poll.link.is_none()
                    };
                    let retained = parsed.retained_bytes
                        + metadata
                            .validators
                            .etag
                            .as_ref()
                            .map_or(0, String::capacity)
                        + metadata
                            .validators
                            .last_modified
                            .as_ref()
                            .map_or(0, String::capacity)
                        + std::mem::size_of::<NotificationCachedPageV63>()
                        + 2 * std::mem::size_of::<usize>();
                    let cache = Arc::new(NotificationCachedPageV63 {
                        rows: Arc::new(parsed.rows),
                        validators: metadata.validators.clone(),
                        exhausted: complete,
                        retained,
                    });
                    (cache, metadata)
                }
                NotificationHttpPageV63::NotModified { poll } => {
                    not_modified += 1;
                    let old = old
                        .filter(|old| {
                            old.validators.etag.is_some() || old.validators.last_modified.is_some()
                        })
                        .ok_or_else(refused)?;
                    let mut metadata = notifications_v63::poll::metadata(&poll, utc_now()?)?;
                    if metadata
                        .validators
                        .etag
                        .as_ref()
                        .is_some_and(|v| old.validators.etag.as_ref() != Some(v))
                        || metadata
                            .validators
                            .last_modified
                            .as_ref()
                            .is_some_and(|v| old.validators.last_modified.as_ref() != Some(v))
                    {
                        return Err(refused());
                    }
                    let complete = if let Some(link) = poll.link {
                        if ordinal < 4 {
                            old.rows.len() < 50
                                && notifications_v63::poll::next_page(
                                    &self.host,
                                    query,
                                    ordinal,
                                    Some(link),
                                )?
                                .is_none()
                        } else {
                            false
                        }
                    } else {
                        old.exhausted
                    };
                    metadata.validators = old.validators.clone();
                    // SAME genuine bounded body allocation, not cached source
                    // key text or a newly deserialized permission. Only current
                    // response pagination metadata receives a tiny wrapper.
                    let page = Arc::new(NotificationCachedPageV63 {
                        rows: Arc::clone(&old.rows),
                        validators: metadata.validators.clone(),
                        exhausted: complete,
                        retained: old.retained,
                    });
                    (page, metadata)
                }
            };
            cached_bytes = cached_bytes
                .checked_add(page.retained)
                .ok_or_else(refused)?;
            // Preflight BEFORE cloning bounded page material. Include prior
            // projection/index capacities and this next full copy, not just
            // the current page or source-key text.
            let projected_bound = page
                .rows
                .iter()
                .map(NotificationRowV63::retained_bytes)
                .sum::<usize>()
                + page.rows.len() * std::mem::size_of::<NotificationRowV63>();
            if cached_bytes
                .checked_add(accumulated.retained_bytes)
                .and_then(|n| {
                    n.checked_add(
                        accumulated.rows.capacity() * std::mem::size_of::<NotificationRowV63>(),
                    )
                })
                .and_then(|n| n.checked_add(projected_bound))
                .is_none_or(|n| n > METADATA)
            {
                return Err(refused());
            }
            let projected = page.rows.as_ref().clone();
            let projected_bytes = projected
                .iter()
                .map(NotificationRowV63::retained_bytes)
                .sum::<usize>()
                + projected.capacity() * std::mem::size_of::<NotificationRowV63>();
            if cached_bytes
                .checked_add(projected_bytes)
                .is_none_or(|n| n > METADATA)
            {
                return Err(refused());
            }
            accumulated.admit(NotificationPageV63 {
                raw_rows: page.rows.len(),
                rows: projected,
                retained_bytes: projected_bytes,
                exhausted: page.exhausted,
            })?;
            exhausted = page.exhausted;
            pages.push(page);
            last_poll.interval_seconds = last_poll.interval_seconds.max(poll.interval_seconds);
            last_poll.retry_seconds = last_poll.retry_seconds.max(poll.retry_seconds);
            last_poll.validators = poll.validators;
            live(token, deadline)?;
            if exhausted {
                break;
            }
        }
        let mut repository_ids = BTreeSet::new();
        let mut omitted = 0usize;
        accumulated.rows.retain(|row| {
            let id = row.repository().id();
            if repository_ids.contains(&id) || repository_ids.len() < REPOSITORIES {
                repository_ids.insert(id);
                true
            } else {
                omitted += 1;
                false
            }
        });
        if let Some(old) = previous {
            // Same old notification ID cannot silently become a different
            // repository/owner/subject when a fresh200 replaces cached rows.
            // A changed title/read timestamp is observation, not new identity.
            for current in &accumulated.rows {
                if old
                    .rows()
                    .iter()
                    .find(|row| row.id() == current.id())
                    .is_some_and(|row| !row.same_original_target_v63(current))
                {
                    return Err(refused());
                }
            }
        }
        let retained = accumulated.retained_bytes
            + accumulated.rows.capacity() * std::mem::size_of::<NotificationRowV63>();
        let remaining = METADATA
            .checked_sub(cached_bytes)
            .and_then(|n| n.checked_sub(retained))
            .filter(|n| *n > 0)
            .ok_or_else(refused)?;
        let original = NotificationRepositoryOriginalV63::from_observed(
            self,
            &identity,
            query,
            &accumulated.rows,
            exhausted && omitted == 0,
            remaining,
        )?;
        let remaining = remaining
            .checked_sub(original.retained_bytes())
            .filter(|n| *n > 0)
            .ok_or_else(refused)?;
        limits.transport.max_metadata_bytes = remaining;
        let mut core = OriginalRepositoryAccessCoreV13::from_started_notifications_v63(
            self,
            original.clone(),
            identity,
            &limits,
            budget,
            token,
        )?;
        for index in 0..original.repositories().len() {
            let observed = core.verify(
                &original.reference(index).expect("actual member index"),
                token,
            )?;
            if observed.repository().permissions.pull != Some(true) {
                return Err(error(GithubErrorKind::Permission,"Current original notification repository read permission unavailable; private result refused"));
            }
        }
        let coverage = NotificationCoverageV63 {
            pages: pages.len(),
            raw_rows: accumulated.raw_rows,
            duplicates: accumulated.duplicates,
            unverified_repository_rows: omitted,
            complete: exhausted && omitted == 0,
            not_modified,
        };
        live(token, deadline)?;
        Ok(NotificationReadAccessV63 {
            core,
            original,
            witness,
            rows: accumulated.rows,
            pages,
            poll: last_poll,
            coverage,
            original_cancel: token.clone(),
            producing_runner: self.runner.clone(),
            deadline,
        })
    }
}
impl NotificationReadAccessV63<'_> {
    pub(crate) fn write_origin_v63(
        &self,
        index: usize,
    ) -> GithubResult<notifications_v63::NotificationWriteOriginV63> {
        live(&self.original_cancel, self.deadline)?;
        notifications_v63::NotificationWriteOriginV63::from_actual(
            &self.producing_runner,
            self.core.identity(),
            self.rows.get(index).ok_or_else(refused)?,
            &self.witness,
            &self.original_cancel,
            self.deadline,
        )
    }
    /// One bounded read of the selected row's latest comment (or subject), on
    /// the SAME runner, token and original deadline, after the repository was
    /// verified readable. GitHub notifications carry no acting user, so this
    /// is the latest commenter, never a claimed notification actor.
    pub(crate) fn latest_actor_v63(
        &self,
        provider: &GithubProvider,
        index: usize,
    ) -> NotificationLatestActorV63 {
        let Some(row) = self.rows.get(index) else {
            return NotificationLatestActorV63::Unavailable("notification not observed");
        };
        let Some((endpoint, is_subject)) =
            notifications_v63::parse::latest_comment_endpoint(&provider.host, row)
        else {
            return NotificationLatestActorV63::Unavailable("no latest comment observed");
        };
        if !provider.runner.same_instance(&self.producing_runner)
            || live(&self.original_cancel, self.deadline).is_err()
        {
            return NotificationLatestActorV63::Unavailable("original read window ended");
        }
        let child = match provider
            .clone()
            .with_scoped_read_window(self.deadline, PAGE)
        {
            Ok(child) => child,
            Err(_) => return NotificationLatestActorV63::Unavailable("read bound unavailable"),
        };
        match child
            .api(&endpoint, "GET", None, None, false, &self.original_cancel)
            .and_then(|value| notifications_v63::parse::latest_actor(&value))
        {
            Ok(actor) => NotificationLatestActorV63::Observed { actor, is_subject },
            Err(_) => NotificationLatestActorV63::Unavailable("latest comment could not be read"),
        }
    }
    /// ONE bounded GraphQL read of the latest actor of every issue or pull
    /// request among the first visible-page rows, on the SAME runner, token
    /// and original deadline, after every repository was verified readable.
    /// No eligible row means no child. A failed read leaves every actor
    /// unknown and says so; it never fails the list.
    pub(crate) fn latest_actors_v63(&self, provider: &GithubProvider) -> NotificationActorsV63 {
        let unknown = |status| NotificationActorsV63 {
            actors: vec![None; self.rows.len()],
            status,
        };
        let Some((query, plan)) = notifications_v63::parse::latest_actors_plan(
            &self.rows,
            notifications_v63::parse::LATEST_ACTOR_ROWS,
        ) else {
            return unknown(NotificationActorsStatusV63::NoneEligible);
        };
        if !provider.runner.same_instance(&self.producing_runner)
            || live(&self.original_cancel, self.deadline).is_err()
        {
            return unknown(NotificationActorsStatusV63::Unavailable);
        }
        let Ok(child) = provider
            .clone()
            .with_scoped_read_window(self.deadline, PAGE)
        else {
            return unknown(NotificationActorsStatusV63::Unavailable);
        };
        let Ok(value) = child.api(
            "graphql",
            "POST",
            Some(serde_json::json!({ "query": query })),
            None,
            false,
            &self.original_cancel,
        ) else {
            return unknown(NotificationActorsStatusV63::Unavailable);
        };
        NotificationActorsV63 {
            actors: plan
                .iter()
                .map(|target| {
                    target.and_then(|(alias, is_subject)| {
                        notifications_v63::parse::latest_actors(&value, alias, is_subject)
                    })
                })
                .collect(),
            status: if self.rows.len() > notifications_v63::parse::LATEST_ACTOR_ROWS {
                NotificationActorsStatusV63::FirstRows(notifications_v63::parse::LATEST_ACTOR_ROWS)
            } else {
                NotificationActorsStatusV63::Read
            },
        }
    }
    pub(crate) fn local_witness_v63(&self) -> &NotificationSourceWitnessV63 {
        &self.witness
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        self.core.identity()
    }
    /// Worker-only preparation input. Not publishable until finish consumes
    /// SAME original Core and its typed local witness after preparation.
    pub(crate) fn rows(&self) -> &[NotificationRowV63] {
        &self.rows
    }
    pub(crate) fn coverage(&self) -> &NotificationCoverageV63 {
        &self.coverage
    }
    pub(crate) fn finish(
        mut self,
        cache: &Cache,
        token: &CancellationToken,
    ) -> GithubResult<Arc<ClosedNotificationReadV63>> {
        if !self.original_cancel.same_instance(token) {
            return Err(refused());
        }
        live(token, self.deadline)?;
        for i in 0..self.original.repositories().len() {
            self.core.verify(
                &self.original.reference(i).expect("actual original member"),
                token,
            )?;
        }
        let repository_metadata_bytes = self.core.metadata_bytes;
        let closed = self.core.close_with_guard(token, || {
            cache
                .validate_notification_source_v63(&self.witness, token, self.deadline)
                .map_err(|_| refused())
        })?;
        live(token, self.deadline)?;
        Ok(Arc::new(ClosedNotificationReadV63 {
            original: self.original,
            witness: self.witness,
            rows: self.rows,
            pages: self.pages,
            poll: self.poll,
            coverage: self.coverage,
            closed,
            repository_metadata_bytes,
        }))
    }
}
#[cfg(test)]
#[path = "notification_publication_v63_tests.rs"]
mod publication_tests;
#[cfg(test)]
#[path = "notification_recovery_v63_tests.rs"]
mod recovery_tests;
#[cfg(test)]
#[path = "notification_access_v63_tests.rs"]
mod tests;
