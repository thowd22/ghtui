//! Genuine read-only Metadata section on the SAME initial producer/Inbox owner.
//! Preparation is worker-only and finishes before original SQL/final UID close.
use super::*;
use crate::{
    app::{App, View},
    github_workspace::{PrKey, PrRow, PrTab},
    personal_pr_entry_v13::PersonalPrPageTicketV13,
    providers::gh::{MetadataDisplayFactsV51, PrContext, PrSummary},
    scoped_pr_observed_tabs_v1::ScopedPrNavigationV1,
};

pub(crate) struct PreparedMetadataSectionV51 {
    opening: Arc<PrCollaborationOpenIntentV17>,
    summary: PrSummary,
    context: PrContext,
    facts: MetadataDisplayFactsV51,
    rows: Vec<PrRow>,
    selected: PrKey,
    navigation: Option<Arc<ScopedPrNavigationV1>>,
    personal: Option<Arc<PersonalPrPageTicketV13>>,
    identity: HostIdentity,
    caption: String,
    cutoff: Instant,
}
impl PreparedMetadataSectionV51 {
    pub(crate) fn matches_original(&self, original: &Arc<PrCollaborationOpenIntentV17>) -> bool {
        Arc::ptr_eq(&self.opening, original)
    }
}
enum MetadataSectionLocalV62 {
    Ordinary(ResourceLocalGuard),
    MembersBlocked {
        target:crate::pr_write_journal::PrWriteTarget,
        parent:Arc<crate::providers::gh::PrResourceParentV1>,
        witness:Box<crate::pr_write_journal::metadata_members_v48::history_v48::MetadataMembersRecoveryWitnessV48>,
    },
}
impl MetadataSectionLocalV62 {
    fn capture(
        database: &Path,
        boundary: PrLocalSourceBoundaryV13<'_>,
        identity: &HostIdentity,
        actual: &mut crate::providers::gh::ActualMetadataTabContextV51,
    ) -> GithubResult<Self> {
        use crate::pr_write_journal::{PrWriteJournal, PrWriteTarget};
        let target =
            PrWriteTarget::new(identity, actual.parent().summary()).map_err(|_| failure())?;
        let journal = PrWriteJournal::open(database).map_err(|_| failure())?;
        // Bounded scalar/8KiB read on the context's already charged transport
        // scratch. Do not impose a256KiB restriction on the ordinary None case.
        let Some(marker) = journal.source_blocking(&target).map_err(|_| failure())? else {
            return ResourceLocalGuard::capture_boundary_v13(
                database,
                boundary,
                identity,
                actual.parent().summary(),
            )
            .map(Self::Ordinary);
        };
        actual.reserve_members_blocked_section_v62()?;
        // The existing256KiB blocked-only debit includes64KiB of fixed
        // statement/map/validation overlap. Before loading/boxing the witness,
        // verify its two padded layouts plus pointer fit that fixed term.
        // Ordinary None never pays this debit or changes its acceptance.
        type Witness = crate::pr_write_journal::metadata_members_v48::history_v48::MetadataMembersRecoveryWitnessV48;
        if std::alloc::Layout::new::<Witness>()
            .pad_to_align()
            .size()
            .checked_mul(2)
            .and_then(|bytes| bytes.checked_add(std::mem::size_of::<Box<Witness>>()))
            .is_none_or(|bytes| bytes > 64 * 1024)
        {
            return Err(failure());
        }
        let origin = journal
            .metadata_members_origin_v48(&target, &marker)
            .map_err(|_| failure())?;
        if !origin.seed.same_object(actual.parent()) {
            return Err(failure());
        }
        let witness = journal
            .prepare_metadata_members_inspection_v48(&target, &marker.publication_reference())
            .map_err(|_| failure())?;
        let local = Self::MembersBlocked {
            target,
            parent: Arc::clone(actual.parent()),
            witness: Box::new(witness),
        };
        local.validate(database, boundary)?;
        Ok(local)
    }
    fn validate(
        &self,
        database: &Path,
        boundary: PrLocalSourceBoundaryV13<'_>,
    ) -> GithubResult<()> {
        match self {
            Self::Ordinary(local) => local.validate_boundary_v13(database, boundary),
            Self::MembersBlocked {
                target,
                parent,
                witness,
            } => ResourceLocalGuard::validate_members_metadata_section_v62(
                database, boundary, target, parent, witness,
            ),
        }
    }
}
pub(crate) fn read(
    opening: Arc<PrCollaborationOpenIntentV17>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<PreparedMetadataSectionV51> {
    let completed = initial_v17::run_metadata_section_v51(
        opening.clone(),
        provider,
        database,
        cancel,
        deadline,
        |mut read| {
            let cutoff = read.effective_cutoff_v51();
            live(cancel, cutoff)?;
            let mut actual = read.take_metadata_section_v51()?;
            let local = MetadataSectionLocalV62::capture(
                database,
                read.boundary,
                read.identity,
                &mut actual,
            )?;
            let rows = actual.prepare_rows(cancel)?;
            let (summary, context, facts) = actual.into_parts();
            let selected = PrKey::from(&summary.pr);
            let (navigation, personal) = if let Some(old) = opening.catalogue_navigation() {
                (
                    Some(Arc::new(
                        ScopedPrNavigationV1::bind(
                            opening.scope(),
                            old.original(),
                            &selected,
                            &summary,
                        )
                        .map_err(|_| failure())?,
                    )),
                    None,
                )
            } else {
                (None, Some(PersonalPrPageTicketV13::new()))
            };
            let identity = read.identity.clone();
            let caption=format!("{} / account {} / {}#{} • Metadata observed • other sections retain prior observations",
                identity.host,identity.account_id.ok_or_else(failure)?,selected.repository,selected.number);
            live(cancel, cutoff)?;
            Ok((
                local,
                PreparedMetadataSectionV51 {
                    opening: opening.clone(),
                    summary,
                    context,
                    facts,
                    rows,
                    selected,
                    navigation,
                    personal,
                    identity,
                    caption,
                    cutoff,
                },
            ))
        },
        |(local, _), boundary| local.validate(database, boundary),
    )?;
    let (_, mut prepared) = completed.material;
    // Successful completed Core only. Actual closing identity is MOVED; there
    // is no post-final-UID SQL/body/formatting/source construction.
    prepared.identity = completed.identity;
    live(cancel, prepared.cutoff)?;
    Ok(prepared)
}
pub(crate) fn install(
    prepared: PreparedMetadataSectionV51,
    app: &mut App,
    owner: &crate::scoped_pr_collaboration_native_v17::ScopedPrCollaborationOwnerV17,
) -> bool {
    if app.view != View::Inbox
        || app.overlay.is_some()
        || app.form.is_some()
        || !matches!(owner.original(),crate::scoped_pr_collaboration_native_v17::CollaborationWorkOriginV78::PrPage(original) if Arc::ptr_eq(original,&prepared.opening))
        || !owner.owns(app)
        || !prepared
            .opening
            .owns_metadata_section_presentation_v266(app)
        || Instant::now() >= prepared.cutoff
    {
        return false;
    }
    let Some(github) = app.github.as_mut() else {
        return false;
    };
    if github.selected.as_ref() != Some(&prepared.selected) {
        return false;
    }
    let Some(page) = github.pr.as_mut() else {
        return false;
    };
    page.details.summary = prepared.summary;
    page.context = Some(prepared.context);
    page.metadata_display_v51 = Some(prepared.facts);
    page.rows = prepared.rows;
    page.tab = PrTab::Metadata;
    github.identity = prepared.identity;
    github.scoped_pr_navigation = prepared.navigation;
    github.personal_pr_page_ticket_v13 = prepared.personal;
    app.context = prepared.caption;
    app.selected = 0;
    app.scroll = 0;
    app.focus = 0;
    app.github_content_focused = true;
    app.hits.clear();
    app.status =
        "Actual Metadata observed • read-only context, not write permission • no publication"
            .into();
    true
}
