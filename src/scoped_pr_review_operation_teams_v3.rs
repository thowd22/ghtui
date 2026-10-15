//! Genuine bounded repository-team selector preparation, not a fake submitted
//! review/author or permission catalogue. Same original source/session/clock.
use super::*;
pub struct PreparedReviewRequestTeamsV3 {
    pub(crate) original: Arc<ScopedReviewIntentV2>,
    pub(crate) roster: Arc<ReviewRequestTeamsV3>,
    pub(crate) labels: Arc<[String]>,
    pub(crate) actors: Arc<[super::super::super::avatars_v7::PreparedActorV7]>,
}
#[expect(
    clippy::too_many_arguments,
    reason = "Keep original source, provider, database, independent read limits, cancellation and deadline explicit"
)]
pub fn read(
    original: Arc<ScopedReviewIntentV2>,
    provider: &GithubProvider,
    database: &Path,
    limits: &ReviewReadLimitsV2,
    resources: &PrResourceReadLimitsV1,
    access: &RepositoryCatalogueAccessLimits,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<PreparedReviewRequestTeamsV3, ScopedReviewFailureV2> {
    clock(cancel, deadline)?;
    limits.validate().map_err(source_failure)?;
    resources.validate().map_err(source_failure)?;
    let issuer = provider.clone().with_catalogue_read_deadline(deadline);
    let mut session = issuer
        .begin_repository_catalogue_access(original.source.source(), access, cancel)
        .map_err(source_failure)?;
    let identity = session.identity().clone();
    original_scope(&original, &identity, cancel, deadline)?;
    let count = limits
        .max_pages
        .checked_add(2)
        .ok_or_else(|| refused("Original team read bound overflow"))?;
    let window = session
        .reserve_measured_external_reads(
            count,
            limits.max_document_bytes,
            8 * 1024 * 1024,
            2,
            cancel,
        )
        .map_err(source_failure)?;
    let measured = issuer
        .clone()
        .with_measured_read_window(window)
        .map_err(source_failure)?;
    let current = session
        .verify(original.navigation.original(), cancel)
        .map_err(source_failure)?;
    let parent = measured
        .pr_inline_original_parent_v1(current, &original.files.observed, resources, cancel)
        .map_err(source_failure)?;
    let local =
        ResourceLocalGuard::capture(database, &original.source, &identity, parent.summary())
            .map_err(source_failure)?;
    let roster = measured
        .review_request_teams_original_v3(&identity, &parent, limits, cancel, deadline)
        .map_err(source_failure)?;
    if !roster.complete() {
        return Err(refused(
            "Team enumeration is incomplete; no partial selected re-request",
        ));
    }
    // A repository Team database ID is not a numeric user/account UID. The
    // genuine team ID is labelled separately in its worker-prepared row.
    let refs = roster
        .teams()
        .iter()
        .map(|team| (team.actor(), None))
        .collect::<Vec<_>>();
    let actors = super::super::super::avatars_v7::batch(&identity.host, &refs, cancel, deadline)
        .map_err(|_| refused("Actual bounded team actor projection unavailable"))?;
    let labels=roster.teams().iter().zip(&actors).map(|(team,actor)|format!("Actual repository team {} • slug {} • numeric ID {} • node {} • eligibility UNKNOWN",
        actor.label(),team.team_slug().unwrap_or("unavailable"),team.id(),team.node())).collect::<Vec<_>>();
    let size = labels
        .iter()
        .try_fold(0usize, |sum, label| sum.checked_add(label.len()))
        .filter(|bytes| *bytes <= 256 * 1024)
        .ok_or_else(|| refused("Actual team labels exceed bounded display"))?;
    let _ = size;
    clock(cancel, deadline)?;
    let current = session
        .observation_v1(parent.repository_id())
        .ok_or_else(|| source_failure(safe_local()))?;
    measured
        .pr_inline_original_parent_v1(current, &original.files.observed, resources, cancel)
        .map_err(source_failure)?;
    session
        .verify(original.navigation.original(), cancel)
        .map_err(source_failure)?;
    session
        .close_with_guard(cancel, || {
            private_clock(cancel, deadline)?;
            local.validate(database, &original.source)?;
            private_clock(cancel, deadline)
        })
        .map_err(source_failure)?;
    clock(cancel, deadline)?;
    Ok(PreparedReviewRequestTeamsV3 {
        original,
        roster,
        labels: labels.into(),
        actors: actors.into(),
    })
}
