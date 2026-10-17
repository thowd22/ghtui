//! Genuine selected-Issue Preview facade. UI selection is descriptive; only
//! the same actual selected-pair Core can produce a seed/plan/Preview.
use crate::{
    pr_issue_links_v87::IssueLinkCommandV87,
    pr_write_journal::{
        issue_link_origin_v87::{
            lineage_v87::{IssueLinkActionBaseV87, IssueLinkTerminalRevisionV87},
            IssueLinkOriginSeedV87,
        },
        PrWriteJournal,
    },
    providers::{
        gh::{
            GithubError, GithubErrorKind, GithubProvider, GithubResult, HostIdentity,
            IssueLinkCatalogueV87, IssueLinkChoiceV87, IssueLinkPlanV87, OriginalIssueLinksV87,
        },
        runner::CancellationToken,
    },
    scoped_pr_collaboration_source_v17::{
        issue_links_phase_v87::{
            IssueLinkPhasePurposeV87, IssueLinkPhaseRequestV87, IssueLinkPhaseV87,
        },
        PrCollaborationSourceV17,
    },
};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Instant,
};

pub(crate) struct IssueLinkIntentV87 {
    pub(crate) source: Arc<PrCollaborationSourceV17>,
    // Keeps the actual producer's weak choice alive. Neither catalogue nor
    // index can substitute for the selected-phase double reads.
    pub(crate) _catalogue: Arc<IssueLinkCatalogueV87>,
    pub(crate) choice: Arc<IssueLinkChoiceV87>,
    pub(crate) command: IssueLinkCommandV87,
}
pub(crate) struct IssueLinkPreviewV87 {
    pub(crate) intent: Arc<IssueLinkIntentV87>,
    pub(crate) actual: Arc<OriginalIssueLinksV87>,
    pub(crate) plan: IssueLinkPlanV87,
    pub(crate) seed: IssueLinkOriginSeedV87,
    pub(crate) values: Arc<BTreeMap<String, String>>,
    pub(crate) key: Arc<str>,
    pub(crate) identity: HostIdentity,
    pub(crate) lines: Arc<[String]>,
    pub(crate) display: Arc<crate::prepared_form_text_v1::PreparedFormTextV1>,
    pub(crate) useful: AtomicBool,
    pub(crate) used: AtomicBool,
    base: IssueLinkActionBaseV87,
    pub(crate) lineage: Option<Arc<IssueLinkTerminalRevisionV87>>,
    pub(crate) needs_new: bool,
}
pub(crate) enum IssuePreviewResultV102 {
    Preview(Arc<IssueLinkPreviewV87>),
    NoChange(crate::providers::gh::IssueNoChangeV102),
}
pub(crate) fn failure() -> GithubError {
    GithubError {
        kind: GithubErrorKind::IdentityChanged,
        message:
            "Exact original manual Issue source/selection/draft changed; no fallback or resend"
                .into(),
    }
}
impl IssueLinkIntentV87 {
    pub(crate) fn choose(
        source: Arc<PrCollaborationSourceV17>,
        catalogue: Arc<IssueLinkCatalogueV87>,
        index: usize,
        command: IssueLinkCommandV87,
    ) -> GithubResult<Arc<Self>> {
        if !source.opening().admits_issue_links_v87(command)
            || !GithubProvider::same_thread_history_parent_v1(source.parent(), catalogue.parent())
        {
            return Err(failure());
        }
        let choice = Arc::new(catalogue.choose(index)?);
        Ok(Arc::new(Self {
            source,
            _catalogue: catalogue,
            choice,
            command,
        }))
    }
}
impl IssueLinkPreviewV87 {
    pub(crate) fn painted(&self, useful: bool) {
        self.useful.store(useful, Ordering::Release);
    }
    pub(crate) fn prepare(
        intent: &Arc<IssueLinkIntentV87>,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
        explicit_new: Option<&Arc<IssueLinkTerminalRevisionV87>>,
    ) -> GithubResult<Arc<Self>> {
        intent.source.with_issue_link_phase_v87(
            IssueLinkPhaseRequestV87 {
                command: intent.command,
                choice: &intent.choice,
                retained: None,
                purpose: IssueLinkPhasePurposeV87::Preview,
            },
            provider,
            database,
            cancel,
            deadline,
            |phase| Self::prepare_phase_v102(intent, phase, database, cancel, explicit_new),
        )
    }
    pub(crate) fn prepare_native_v102(
        intent: &Arc<IssueLinkIntentV87>,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<IssuePreviewResultV102> {
        use crate::scoped_pr_collaboration_source_v17::issue_links_phase_v87::IssuePreviewCallbacksV102;
        intent.source.with_issue_link_native_preview_v102(
            IssueLinkPhaseRequestV87 {
                command: intent.command,
                choice: &intent.choice,
                retained: None,
                purpose: IssueLinkPhasePurposeV87::Preview,
            },
            provider,
            database,
            cancel,
            deadline,
            IssuePreviewCallbacksV102 {
                ready: |phase: IssueLinkPhaseV87<'_>| {
                    Self::prepare_phase_v102(intent, phase, database, cancel, None)
                        .map(IssuePreviewResultV102::Preview)
                },
                no_change: |proof| Ok(IssuePreviewResultV102::NoChange(proof)),
            },
        )
    }
    fn prepare_phase_v102(
        intent: &Arc<IssueLinkIntentV87>,
        phase: IssueLinkPhaseV87<'_>,
        database: &Path,
        cancel: &CancellationToken,
        explicit_new: Option<&Arc<IssueLinkTerminalRevisionV87>>,
    ) -> GithubResult<Arc<Self>> {
        let local = phase.local(database)?;
        let current = IssueLinkOriginSeedV87::from_producer(
            &intent.source,
            phase.actual(),
            phase.plan(),
            database,
        )
        .map_err(|_| failure())?;
        let base = IssueLinkActionBaseV87::from_producer(&current, &phase, database)
            .map_err(|_| failure())?;
        let journal = PrWriteJournal::open(database).map_err(|_| failure())?;
        let lineage = journal
            .issue_link_action_revision_v87(&base)
            .map_err(|_| failure())?;
        let (seed, needs_new) = match (explicit_new, &lineage) {
            (Some(expected), Some(actual)) => {
                journal
                    .validate_issue_link_revision_v87(&base, expected)
                    .map_err(|_| failure())?;
                (
                    IssueLinkOriginSeedV87::from_revision_v87(current, &base, actual)
                        .map_err(|_| failure())?,
                    false,
                )
            }
            (None, Some(_)) => (current, true),
            (None, None) => (current, false),
            _ => return Err(failure()),
        };
        // Actual original + origin/V52/identity/value/doc stack/Arc moves
        // fit the same phase model bound before string/Vec/Arc copies.
        phase
            .actual()
            .retained_bound_v87()
            .checked_add(256 * 1024)
            .filter(|n| *n <= 4 * 1024 * 1024)
            .ok_or_else(failure)?;
        let values = Arc::new(seed.values().map_err(|_| failure())?);
        let key = Arc::from(seed.draft_key());
        let lines = if needs_new {
            let terminal = lineage.as_ref().ok_or_else(failure)?;
            vec!["NEEDS NEW ACTION • no write or replay".into(),terminal.label().into(),
                    format!("Original terminal attempt #{}",terminal.reference().attempt_id),
                    "Choose New action explicitly for another current read; old protected body/reference remains inspect-only".into()].into()
        } else {
            phase.actual().prepare_preview_lines_v87(phase.plan())?
        };
        let display = crate::prepared_form_text_v1::PreparedFormTextV1::prepare(
            &lines,
            cancel,
            phase.deadline(),
        )
        .map_err(|_| failure())?;
        let (actual, plan, identity) = phase.finish(|| {
            local.validate_boundary_v13(database, intent.source.local_boundary())?;
            if let Some(revision) = &lineage {
                journal
                    .validate_issue_link_revision_v87(&base, revision)
                    .map_err(|_| failure())?;
            }
            Ok(())
        })?;
        Ok(Arc::new(Self {
            intent: Arc::clone(intent),
            actual,
            plan,
            seed,
            values,
            key,
            identity,
            lines,
            display,
            useful: AtomicBool::new(false),
            used: AtomicBool::new(false),
            base,
            lineage,
            needs_new,
        }))
    }
    pub(crate) fn bind_fresh_seed(
        &self,
        current: IssueLinkOriginSeedV87,
        phase: &IssueLinkPhaseV87<'_>,
        database: &Path,
    ) -> GithubResult<IssueLinkOriginSeedV87> {
        if self.needs_new {
            return Err(failure());
        }
        let base = IssueLinkActionBaseV87::from_producer(&current, phase, database)
            .map_err(|_| failure())?;
        if let Some(revision) = &self.lineage {
            let journal = PrWriteJournal::open(database).map_err(|_| failure())?;
            journal
                .validate_issue_link_revision_v87(&self.base, revision)
                .map_err(|_| failure())?;
            journal
                .validate_issue_link_revision_v87(&base, revision)
                .map_err(|_| failure())?;
            IssueLinkOriginSeedV87::from_revision_v87(current, &base, revision)
                .map_err(|_| failure())
        } else {
            Ok(current)
        }
    }
}
