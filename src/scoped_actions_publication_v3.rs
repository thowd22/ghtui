//! Closed original-source Cancel preparation. A closed Preview is display
//! material ONLY: no deadline/read ledger survives as Confirm authority.
use crate::{
    actions_workspace::{
        ActionsCommand, ActionsPlan, ActionsView, ActionsWorkspace, RunPage, ScopedRun,
    },
    prepared_form_text_v1::PreparedFormTextV1,
    providers::{gh::*, runner::CancellationToken},
    scoped_actions_v1::{ScopedActionsPageV1, ScopedActionsSourceV1},
    storage::{Cache, Scope},
};
use std::{
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Instant,
};
mod history_v3;
pub(crate) use history_v3::inspect_with_acceptance_v5;
pub(crate) use history_v3::ScopedActionsHistoryV3;

fn refuse(message: &'static str) -> GithubError {
    GithubError {
        kind: GithubErrorKind::InvalidTarget,
        message: message.into(),
    }
}
pub(crate) fn clock(cancel: &CancellationToken, deadline: Instant) -> GithubResult<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        return Err(refuse(
            "Original Actions operation cancelled/expired; no renewed purpose",
        ));
    }
    Ok(())
}
fn file(path: &Path) -> GithubResult<(u64, u64)> {
    let value = std::fs::symlink_metadata(path)
        .map_err(|_| refuse("Original Actions database unavailable"))?;
    // SAFETY: Linux/macOS effective UID query has no side effects.
    let uid = unsafe { libc::geteuid() };
    if !value.is_file() || value.uid() != uid || value.nlink() != 1 || value.mode() & 0o077 != 0 {
        return Err(refuse(
            "Original Actions database is no longer its private original file",
        ));
    }
    Ok((value.dev(), value.ino()))
}
pub(crate) struct LocalV3 {
    path: PathBuf,
    file: (u64, u64),
}
impl LocalV3 {
    pub(crate) fn owned_bytes_v6(&self) -> usize {
        std::mem::size_of::<Self>() + self.path.capacity()
    }
    pub(crate) fn capture(database: &Path, source: &ScopedActionsSourceV1) -> GithubResult<Self> {
        source.validate_local(database)?;
        let path = std::fs::canonicalize(database)
            .map_err(|_| refuse("Original Actions database unavailable"))?;
        let value = Self {
            file: file(&path)?,
            path,
        };
        value.validate(database, source)?;
        Ok(value)
    }
    pub(crate) fn validate(
        &self,
        database: &Path,
        source: &ScopedActionsSourceV1,
    ) -> GithubResult<()> {
        if std::fs::canonicalize(database)
            .map_err(|_| refuse("Original Actions database unavailable"))?
            != self.path
            || file(database)? != self.file
        {
            return Err(refuse(
                "Original Actions database identity changed; nothing authorized",
            ));
        }
        source.validate_local(database)?;
        if file(database)? != self.file {
            return Err(refuse(
                "Original Actions database changed during SQL validation",
            ));
        }
        Ok(())
    }
    pub(crate) fn validate_owned_v6(
        &self,
        database: &Path,
        source: &ScopedActionsSourceV1,
        journal: &crate::actions_write_journal::ActionsWriteJournal,
        claim: &crate::actions_write_journal::ActionsClaimStampV3,
        result: Option<(
            crate::actions_write_journal::ActionsWriteState,
            &crate::actions_write_journal::ActionsReceipt,
        )>,
    ) -> GithubResult<()> {
        if std::fs::canonicalize(database)
            .map_err(|_| refuse("Original Actions database unavailable"))?
            != self.path
            || file(database)? != self.file
        {
            return Err(refuse("Original Actions database was replaced"));
        }
        source.validate_owned_publication_local_v4(database, journal, claim, result)?;
        if file(database)? != self.file {
            return Err(refuse(
                "Original Actions database changed during validation",
            ));
        }
        Ok(())
    }
    pub(crate) fn validate_run_history_v8(
        &self,
        database: &Path,
        source: &ScopedActionsSourceV1,
        journal: &crate::actions_write_journal::ActionsWriteJournal,
        proof: &crate::actions_write_journal::ActionsOwnedRunHistoryV8,
    ) -> GithubResult<()> {
        if std::fs::canonicalize(database)
            .map_err(|_| refuse("Original Run history cache unavailable"))?
            != self.path
            || file(database)? != self.file
        {
            return Err(refuse("Original Run history database replaced"));
        }
        source.validate_owned_run_history_v8(database, journal, proof)?;
        if file(database)? != self.file {
            return Err(refuse("Original Run history database changed"));
        }
        Ok(())
    }
    pub(crate) fn validate_dispatch_history_v7(
        &self,
        database: &Path,
        source: &ScopedActionsSourceV1,
        journal: &crate::actions_write_journal::ActionsWriteJournal,
        proof: &crate::actions_write_journal::ActionsOwnedDispatchHistoryV7,
    ) -> GithubResult<()> {
        if std::fs::canonicalize(database)
            .map_err(|_| refuse("Original Dispatch history cache unavailable"))?
            != self.path
            || file(database)? != self.file
        {
            return Err(refuse("Original Dispatch history database replaced"));
        }
        source.validate_owned_dispatch_history_v7(database, journal, proof)?;
        if file(database)? != self.file {
            return Err(refuse("Original Dispatch history database changed"));
        }
        Ok(())
    }
    pub(crate) fn validate_workflow_history_v6(
        &self,
        database: &Path,
        source: &ScopedActionsSourceV1,
        journal: &crate::actions_write_journal::ActionsWriteJournal,
        proof: &crate::actions_write_journal::ActionsOwnedWorkflowHistoryV6,
    ) -> GithubResult<()> {
        if std::fs::canonicalize(database)
            .map_err(|_| refuse("Original history cache unavailable"))?
            != self.path
            || file(database)? != self.file
        {
            return Err(refuse("Original history database replaced"));
        }
        source.validate_owned_workflow_history_v6(database, journal, proof)?;
        if file(database)? != self.file {
            return Err(refuse("Original history database changed"));
        }
        Ok(())
    }
    pub(crate) fn validate_workflow_branch_transition_v9(
        &self,
        database: &Path,
        source: &ScopedActionsSourceV1,
        journal: &crate::actions_write_journal::ActionsWriteJournal,
        branch: &crate::scoped_actions_workflow_edit_v9::WorkflowPublishedBranchV9,
    ) -> GithubResult<()> {
        if std::fs::canonicalize(database)
            .map_err(|_| refuse("Original workflow database unavailable"))?
            != self.path
            || file(database)? != self.file
        {
            return Err(refuse("Original workflow database replaced"));
        }
        source.validate_workflow_branch_transition_v9(database, journal, branch)?;
        if file(database)? != self.file {
            return Err(refuse("Original workflow database changed"));
        }
        Ok(())
    }
}
/// Privately captured from an ACTUAL existing page/source/run. No public DTO,
/// old form labels or serialized draft can reconstruct this original intent.
pub struct ScopedActionsIntentV3 {
    source: Arc<ScopedActionsSourceV1>,
    page: Arc<ScopedActionsPageV1>,
    original: Arc<ScopedActionsRunTargetV1>,
    epoch: u64,
    key: Arc<str>,
    command: ScopedRunCommandV4,
}
/// Closed selectors retain an actual page-produced job. No deserialization or
/// scalar job-ID constructor can mint this original selection.
enum ScopedRunCommandV4 {
    Cancel,
    ForceCancel,
    DeleteRun,
    DeleteLogs,
    RerunAll,
    RerunFailed,
    RerunJob(Arc<ScopedActionsJobTargetV1>),
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ScopedRunOptionsV4 {
    pub(crate) debug_logging: bool,
    pub(crate) force_acknowledged: bool,
}
impl ScopedRunOptionsV4 {
    pub(crate) fn values(
        self,
        command: ActionsCommand,
    ) -> std::collections::BTreeMap<String, String> {
        if matches!(
            command,
            ActionsCommand::RerunAll | ActionsCommand::RerunFailed | ActionsCommand::RerunJob
        ) {
            [("debug".into(), self.debug_logging.to_string())]
                .into_iter()
                .collect()
        } else {
            Default::default()
        }
    }
}
impl std::fmt::Debug for ScopedActionsIntentV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedActionsIntentV3(<actual original Cancel intent>)")
    }
}
impl ScopedActionsIntentV3 {
    pub(crate) fn capture(page: Arc<ScopedActionsPageV1>, epoch: u64) -> Result<Arc<Self>, String> {
        Self::capture_command_v4(page, epoch, ActionsCommand::Cancel)
    }
    pub(crate) fn capture_command_v4(
        page: Arc<ScopedActionsPageV1>,
        epoch: u64,
        requested: ActionsCommand,
    ) -> Result<Arc<Self>, String> {
        let command=match requested {
            ActionsCommand::Cancel=>ScopedRunCommandV4::Cancel,
            ActionsCommand::ForceCancel=>ScopedRunCommandV4::ForceCancel,
            ActionsCommand::DeleteRun=>ScopedRunCommandV4::DeleteRun,
            ActionsCommand::DeleteLogs=>ScopedRunCommandV4::DeleteLogs,
            ActionsCommand::RerunAll=>ScopedRunCommandV4::RerunAll,
            ActionsCommand::RerunFailed=>ScopedRunCommandV4::RerunFailed,
            ActionsCommand::RerunJob=>ScopedRunCommandV4::RerunJob(page.selected_job_for_publication_v4()
                .ok_or("Select an actual exact-attempt job before rerunning it; URL numbers grant no target")?),
            _=>return Err("No scoped original run-command adapter".into()),
        };
        let original = page
            .run()
            .cloned()
            .ok_or("Open an actual run attempt before Cancel")?;
        if original.node_v2().is_none() || original.repository_node().is_none() {
            return Err("Actual run/repository node unavailable; no guessed write target".into());
        }
        let source = page.source().clone();
        let key: Arc<str> = format!(
            "scoped-actions-v3/repository/{}/run/{}/attempt/{}/head/{}",
            original.repository_id(),
            original.run().id,
            original.run().attempt,
            original.run().head_sha
        )
        .into();
        if key.len() > 1024 {
            return Err("Original Actions draft key exceeds1KiB".into());
        }
        Ok(Arc::new(Self {
            source,
            page,
            original,
            epoch,
            key,
            command,
        }))
    }
    pub(crate) fn command(&self) -> ActionsCommand {
        match &self.command {
            ScopedRunCommandV4::Cancel => ActionsCommand::Cancel,
            ScopedRunCommandV4::ForceCancel => ActionsCommand::ForceCancel,
            ScopedRunCommandV4::DeleteRun => ActionsCommand::DeleteRun,
            ScopedRunCommandV4::DeleteLogs => ActionsCommand::DeleteLogs,
            ScopedRunCommandV4::RerunAll => ActionsCommand::RerunAll,
            ScopedRunCommandV4::RerunFailed => ActionsCommand::RerunFailed,
            ScopedRunCommandV4::RerunJob(_) => ActionsCommand::RerunJob,
        }
    }
    pub(crate) fn selected_job(&self) -> Option<&Arc<ScopedActionsJobTargetV1>> {
        if let ScopedRunCommandV4::RerunJob(job) = &self.command {
            Some(job)
        } else {
            None
        }
    }
    pub(crate) fn action(&self, options: ScopedRunOptionsV4) -> RunMutation {
        match &self.command {
            ScopedRunCommandV4::Cancel => RunMutation::Cancel,
            ScopedRunCommandV4::ForceCancel => RunMutation::ForceCancel,
            ScopedRunCommandV4::DeleteRun => RunMutation::DeleteRun,
            ScopedRunCommandV4::DeleteLogs => RunMutation::DeleteLogs,
            ScopedRunCommandV4::RerunAll => RunMutation::RerunAll {
                debug: options.debug_logging,
            },
            ScopedRunCommandV4::RerunFailed => RunMutation::RerunFailed {
                debug: options.debug_logging,
            },
            ScopedRunCommandV4::RerunJob(job) => RunMutation::RerunJob {
                job_id: job.id(),
                debug: options.debug_logging,
            },
        }
    }
    pub(crate) fn source(&self) -> &Arc<ScopedActionsSourceV1> {
        &self.source
    }
    pub(crate) fn page(&self) -> &Arc<ScopedActionsPageV1> {
        &self.page
    }
    pub(crate) fn original(&self) -> &Arc<ScopedActionsRunTargetV1> {
        &self.original
    }
    pub(crate) fn epoch(&self) -> u64 {
        self.epoch
    }
    pub(crate) fn key(&self) -> &str {
        &self.key
    }
}
pub struct OpenedActionsComposerV3 {
    intent: Arc<ScopedActionsIntentV3>,
    local: LocalV3,
    target: Arc<ScopedActionsRunTargetV1>,
    identity: HostIdentity,
    lines: Vec<String>,
    display: Arc<PreparedFormTextV1>,
    retained: usize,
    history: Option<Arc<ScopedActionsHistoryV3>>,
    force_candidate_v5: Option<crate::actions_write_journal::ActionsPublicationReference>,
    form_seed: std::sync::Mutex<Option<crate::forms::Form>>,
}
impl std::fmt::Debug for OpenedActionsComposerV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OpenedActionsComposerV3(<closed display, not approval>)")
    }
}
pub struct ScopedActionsPreviewV3 {
    opened: Arc<OpenedActionsComposerV3>,
    target: Arc<ScopedActionsRunTargetV1>,
    plan: Arc<ActionsPlan>,
    display: Arc<PreparedFormTextV1>,
    attempted: AtomicBool,
    options: ScopedRunOptionsV4,
}
/// One NEW explicit Confirm purpose, captured before queue. It borrows only
/// closed Preview display; it never reuses/renews the Preview's sealed reads.
pub struct ScopedActionsConfirmV3 {
    preview: Arc<ScopedActionsPreviewV3>,
    cancel: CancellationToken,
    deadline: Instant,
    started: AtomicBool,
}
impl std::fmt::Debug for ScopedActionsConfirmV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedActionsConfirmV3(<one explicit original queued attempt>)")
    }
}
impl ScopedActionsConfirmV3 {
    pub(crate) fn capture(
        preview: Arc<ScopedActionsPreviewV3>,
        cancel: &CancellationToken,
        original_deadline: Instant,
    ) -> Result<Arc<Self>, String> {
        clock(cancel, original_deadline).map_err(|_| "Original Confirm phase ended")?;
        preview.claim_attempt()?;
        Ok(Arc::new(Self {
            preview,
            cancel: cancel.clone(),
            deadline: original_deadline,
            started: AtomicBool::new(false),
        }))
    }
    pub(crate) fn preview(&self) -> &Arc<ScopedActionsPreviewV3> {
        &self.preview
    }
    pub(crate) fn cancellation(&self) -> &CancellationToken {
        &self.cancel
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }
    pub(crate) fn start(&self) -> GithubResult<()> {
        clock(&self.cancel, self.deadline)?;
        self.started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| ())
            .map_err(|_| refuse("Original Confirm already attempted; no automatic retry"))
    }
}
impl std::fmt::Debug for ScopedActionsPreviewV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedActionsPreviewV3(<closed display, no reusable API clock>)")
    }
}
impl OpenedActionsComposerV3 {
    pub(crate) fn force_candidate_v5(
        &self,
    ) -> Option<&crate::actions_write_journal::ActionsPublicationReference> {
        self.force_candidate_v5.as_ref()
    }
    pub(crate) fn history(&self) -> Option<&Arc<ScopedActionsHistoryV3>> {
        self.history.as_ref()
    }
    pub(crate) fn intent(&self) -> &Arc<ScopedActionsIntentV3> {
        &self.intent
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub(crate) fn lines(&self) -> &[String] {
        &self.lines
    }
    pub(crate) fn display(&self) -> &Arc<PreparedFormTextV1> {
        &self.display
    }
    pub(crate) fn validate_local(&self, database: &Path) -> GithubResult<()> {
        self.local.validate(database, &self.intent.source)
    }
    pub(crate) fn validate_owned_publication_local_v4(
        &self,
        database: &Path,
        journal: &crate::actions_write_journal::ActionsWriteJournal,
        claim: &crate::actions_write_journal::ActionsClaimStampV3,
        result: Option<(
            crate::actions_write_journal::ActionsWriteState,
            &crate::actions_write_journal::ActionsReceipt,
        )>,
    ) -> GithubResult<()> {
        if std::fs::canonicalize(database)
            .map_err(|_| refuse("Original Actions database unavailable"))?
            != self.local.path
            || file(database)? != self.local.file
        {
            return Err(refuse("Original Actions database was replaced"));
        }
        self.intent
            .source
            .validate_owned_publication_local_v4(database, journal, claim, result)?;
        if file(database)? != self.local.file {
            return Err(refuse(
                "Original Actions database changed during validation",
            ));
        }
        Ok(())
    }
    pub(crate) fn take_form_seed(&self) -> Result<crate::forms::Form, String> {
        self.form_seed
            .lock()
            .map_err(|_| "Original Form seed unavailable")?
            .take()
            .ok_or_else(|| "Original Form seed already consumed".into())
    }
}
impl ScopedActionsPreviewV3 {
    pub(crate) fn options(&self) -> ScopedRunOptionsV4 {
        self.options
    }
    pub(crate) fn opened(&self) -> &Arc<OpenedActionsComposerV3> {
        &self.opened
    }
    pub(crate) fn plan(&self) -> &Arc<ActionsPlan> {
        &self.plan
    }
    pub(crate) fn target(&self) -> &Arc<ScopedActionsRunTargetV1> {
        &self.target
    }
    pub(crate) fn display(&self) -> &Arc<PreparedFormTextV1> {
        &self.display
    }
    pub(crate) fn available(&self) -> bool {
        !self.attempted.load(Ordering::Acquire)
    }
    pub(crate) fn claim_attempt(&self) -> Result<(), String> {
        self.attempted
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| ())
            .map_err(|_| {
                "This exact Preview already attempted; inspect its original protected result".into()
            })
    }
}
pub(crate) fn access_limits() -> RepositoryCatalogueAccessLimits {
    let mut limits = RepositoryCatalogueAccessLimits::default();
    limits.transport.max_response_bytes = 2 * 1024 * 1024;
    limits.transport.max_document_bytes = 64 * 1024;
    limits.max_repository_reads = 2;
    limits
}
pub(crate) fn scope(source: &ScopedActionsSourceV1, identity: &HostIdentity) -> GithubResult<()> {
    if Scope::github(&identity.host, identity.account_id)
        .ok()
        .as_ref()
        != Some(source.original().scope())
    {
        return Err(refuse("Original Actions numeric principal changed"));
    }
    Ok(())
}
pub(crate) fn same_run(a: &ScopedActionsRunTargetV1, b: &ScopedActionsRunTargetV1) -> bool {
    a.same_watch_pin_v2(b)
        && a.run().status == b.run().status
        && a.run().conclusion == b.run().conclusion
}
fn tiny_workspace(
    identity: &HostIdentity,
    target: &ScopedActionsRunTargetV1,
) -> GithubResult<ActionsWorkspace> {
    // PRE-CLONE admission over actual capacities from the same canonical model.
    // Fixed Cancel cohort has no log/archive/job/setting body or editor values.
    if crate::actions_workspace::scoped_read_v1::run_owned_bytes(target.run()) > 128 * 1024
        || identity.host.capacity() + identity.login.capacity() + identity.gh_version.capacity()
            > 16 * 1024
    {
        return Err(refuse(
            "Original Cancel metadata exceeds128KiB/16KiB identity bounds; no full body clone",
        ));
    }
    let repository = target.repository();
    let mut workspace = ActionsWorkspace::new(identity.clone(), vec![repository.into()]);
    workspace.view = ActionsView::Run;
    workspace.runs = vec![ScopedRun {
        repository: repository.into(),
        run: target.run().clone(),
    }];
    workspace.run = Some(RunPage {
        repository: repository.into(),
        attempt: RunAttempt {
            run: target.run().clone(),
            jobs: vec![],
            definition_note: "Actual original run; no job/log bodies copied into Cancel".into(),
        },
        selected_job: None,
        logs: None,
        logs_skipped_bytes: 0,
        annotations: vec![],
    });
    Ok(workspace)
}
fn identity_bytes(identity: &HostIdentity) -> usize {
    identity.host.capacity() + identity.login.capacity() + identity.gh_version.capacity()
}
fn owned_lines(lines: &Vec<String>) -> usize {
    lines.capacity() * std::mem::size_of::<String>()
        + lines.iter().map(String::capacity).sum::<usize>()
}
/// Worker-only. SAME original prequeue token/deadline, one pool; all actual
/// model/layout allocations finish before original SQL/final UID close.
pub(crate) fn open(
    intent: Arc<ScopedActionsIntentV3>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<Arc<OpenedActionsComposerV3>> {
    clock(cancel, deadline)?;
    let local = LocalV3::capture(database, &intent.source)?;
    let phase = provider.clone().with_catalogue_read_deadline(deadline);
    let mut session = phase.begin_repository_catalogue_access(
        intent.source.original().source(),
        &access_limits(),
        cancel,
    )?;
    scope(&intent.source, session.identity())?;
    let window = session.reserve_measured_external_reads(15, 512 * 1024, 1024 * 1024, 2, cancel)?;
    let measured = phase.clone().with_measured_read_window(window)?;
    session.verify(intent.source.reference(), cancel)?;
    let current = session
        .observed_original_v1(intent.source.reference().id)
        .ok_or_else(|| refuse("Actual original Actions repository unavailable"))?;
    if let Some(pr) = intent.source.pr_origin_v2() {
        pr.fresh_parent(&measured, session.identity(), current, cancel)?;
    }
    let target = measured.scoped_actions_run_snapshot_v3(current, &intent.original, cancel)?;
    let history = if intent.command() == ActionsCommand::ForceCancel {
        None
    } else {
        history_v3::restore(
            &intent,
            &local,
            &measured,
            session.identity(),
            &target,
            database,
            cancel,
            deadline,
        )?
    };
    // Reuse exact existing Plan preparation/eligibility without retaining it as
    // an unrequested Preview or making public display strings a capability.
    let mut workspace = tiny_workspace(session.identity(), &target)?;
    if let Some(job) = intent.selected_job() {
        if !job.run().same_watch_pin_v2(&target) {
            return Err(refuse("Original selected job/run pin changed"));
        }
        workspace
            .run
            .as_mut()
            .expect("actual tiny run")
            .selected_job = Some(job.id());
    }
    let options = ScopedRunOptionsV4::default();
    let candidate = if history.is_none() {
        Some(ActionsPlan::prepare(
            &workspace,
            intent.command(),
            &options.values(intent.command()),
            &measured,
            cancel,
        )?)
    } else {
        None
    };
    let force_candidate_proof_v5 = if let Some(candidate) = &candidate {
        let descriptor = if intent.command() == ActionsCommand::Cancel {
            candidate.scoped_cancel_descriptor_v3(&measured, &intent.source, &target)?
        } else {
            candidate.scoped_run_descriptor_v4(
                &measured,
                &intent.source,
                &target,
                intent.selected_job(),
                intent.action(options),
            )?
        };
        if intent.command() == ActionsCommand::ForceCancel {
            let journal = crate::actions_write_journal::ActionsWriteJournal::open(database)
                .map_err(|_| refuse("Original escalation journal unavailable"))?;
            journal.prepare_force_escalation_v5(&measured, &intent.source, &target,
                &descriptor.target, &descriptor.intent).map_err(|_| refuse("Force escalation needs exactly one accepted normal Cancel or no blocker; ambiguous/unknown protections remain"))?
        } else {
            None
        }
    } else {
        None
    };
    let force_candidate_v5 = force_candidate_proof_v5
        .as_ref()
        .map(|proof| proof.metadata().cancel);
    let mut lines=vec![
        "CANCEL ORIGINAL ACTIONS RUN".into(),
        format!("Numeric repository {} • run {} • attempt {}",target.repository_id(),target.run().id,target.run().attempt),
        format!("Current repository: {}",target.repository()),
        format!("Run head: {}",target.run().head_sha),
        format!("Observed state: {} / {}",target.run().status,target.run().conclusion),
        "Preview is read-only. Only a NEW explicit Confirm can attempt Cancel after fresh original source/account/state checks.".into(),
        "GitHub may accept cancellation before the run stops; acceptance is never claimed as completed cancellation.".into(),
    ];
    if matches!(
        intent.command(),
        ActionsCommand::ForceCancel | ActionsCommand::DeleteRun | ActionsCommand::DeleteLogs
    ) {
        lines[0] = format!(
            "{} • ORIGINAL WHOLE RUN",
            intent.command().label().to_uppercase()
        );
        lines[5] = "Only a NEW explicit Confirm attempts this exact whole-run effect after fresh original source/account/state checks.".into();
        lines[6] = match intent.command() {
            ActionsCommand::ForceCancel => "Force cancellation bypasses always conditions. Choose force_ack=true explicitly; acceptance is not completed cancellation.".into(),
            ActionsCommand::DeleteRun => "PERMANENT whole-run deletion includes all attempts and artifacts, not just the selected attempt. HTTP204 means accepted; a404 is unavailable, NOT proof of deletion or who caused it.".into(),
            _ => "Delete ALL logs for this whole run, not only the displayed job/attempt. The API exposes no deleted-log flag. Acceptance-only acknowledgement is separate from effect confirmation.".into(),
        };
        if let Some(reference) = &force_candidate_v5 {
            lines.push(format!("Explicit escalation of accepted normal Cancel record {} • the old Cancel remains unchanged/protected", reference.attempt_id));
        }
    } else if intent.command() != ActionsCommand::Cancel {
        lines[0] = format!(
            "{} • ORIGINAL ACTIONS INTENT",
            intent.command().label().to_uppercase()
        );
        lines[5]="Preview is read-only. Only one NEW explicit Confirm can request this exact rerun after fresh source/account/state checks.".into();
        lines[6]="Rerun acceptance or a new observed attempt is NOT workflow success or proof of who caused it. Original triggering-actor privileges and original event SHA/ref apply; dependent jobs may rerun.".into();
        if let Some(job) = intent.selected_job() {
            // The exact selected target precedes verbose warnings so it is
            // visible even in the useful 80x24 initial PR intent.
            lines.insert(
                3,
                format!(
                    "Original selected actual job database ID {} • URL numbers are not targets",
                    job.id()
                ),
            );
            let label = intent
                .page
                .original_job_label_v4(job)
                .ok_or_else(|| refuse("Actual selected job label/member no longer agrees"))?;
            lines.push(if label.len() <= 4096 {
                format!("Original observed job name: {label}")
            } else {
                "Original job name exceeds4KiB display bound; exact numeric target remains above"
                    .into()
            });
        }
        let trigger = &target.run().triggering_actor;
        lines.push(format!("Original trigger actor: {} • numeric actor identity unavailable in this canonical run model • rerun requester observed at Open: @{}",
            trigger.label(),session.identity().login));
    }
    if let Some(pr) = intent.source.pr_origin_v2() {
        let original = pr.durable_pr_context_v3();
        lines.push(format!(
            "PR #{} head {} • distinct run head above",
            original.number, original.head_sha
        ));
    }
    drop(candidate);
    let seed_bytes = crate::actions_workspace::scoped_read_v1::retained_bytes(&workspace)
        .map_err(|_| refuse("Original Form seed exceeds shared bounded model"))?;
    // Full actual Form/workspace construction is BEFORE final source/SQL/UID.
    // The post-close install takes this exact seed and replaces its purpose O(1).
    let form_seed = if intent.command() == ActionsCommand::Cancel {
        crate::forms::Form::actions_original_shell_v3(workspace)
    } else {
        crate::forms::Form::actions_original_shell_v4(
            workspace,
            intent.command(),
            history.is_some(),
        )
    }
    .map_err(|_| refuse("Original Form presentation seed unavailable"))?;
    let display = if let Some(history) = &history {
        lines.clear();
        history.display().clone()
    } else {
        PreparedFormTextV1::prepare(&lines, cancel, deadline)
            .map_err(|_| refuse("Original Cancel display exceeds shared presentation bounds"))?
    };
    let mut identity = session.identity().clone();
    let mut retained = std::mem::size_of::<OpenedActionsComposerV3>()
        + 2 * std::mem::size_of::<usize>()
        + target.owned_bytes_v1()
        + owned_lines(&lines)
        + display.retained_bytes()
        + identity_bytes(&identity)
        + local.path.capacity()
        + intent.key.len()
        + 2 * std::mem::size_of::<usize>();
    retained = retained
        .saturating_add(seed_bytes)
        .saturating_add(std::mem::size_of::<crate::forms::Form>() + 1024);
    retained = retained.saturating_add(history.as_ref().map_or(0, |history| history.owned_bytes()));
    retained = retained.saturating_add(force_candidate_v5.as_ref().map_or(0, |reference| {
        std::mem::size_of::<crate::actions_write_journal::ActionsPublicationReference>()
            + reference.fingerprint_sha256.capacity()
    }));
    // The bounded old marker is temporary proof, NOT retained authority in
    // Opened. Its128KiB metadata plus parser/clone/headroom charge is explicit.
    let force_peak = if force_candidate_proof_v5.is_some() {
        512 * 1024
    } else {
        0
    };
    if retained > 8 * 1024 * 1024
        || retained + display.preparation_peak_bound() + force_peak > 16 * 1024 * 1024
    {
        return Err(refuse(
            "Original Cancel open preparation exceeds actual retained/transient bounds",
        ));
    }
    let closing = measured.scoped_actions_run_snapshot_v3(
        session
            .observed_original_v1(intent.source.reference().id)
            .ok_or_else(|| refuse("Original repository unavailable"))?,
        &target,
        cancel,
    )?;
    if !same_run(&closing, &target) {
        return Err(refuse(
            "Original run changed while opening Cancel; open its current read again",
        ));
    }
    drop(closing);
    if let Some(pr) = intent.source.pr_origin_v2() {
        pr.fresh_parent(
            &measured,
            session.identity(),
            session
                .observed_original_v1(intent.source.reference().id)
                .ok_or_else(|| refuse("Original repository unavailable"))?,
            cancel,
        )?;
    }
    session.verify(intent.source.reference(), cancel)?;
    let closed = session.close_with_guard(cancel, || {
        local.validate(database, &intent.source)?;
        if let Some(history) = &history {
            history
                .validate(database)
                .map_err(|_| refuse("Original history changed before final principal"))?;
        }
        if let Some(proof) = &force_candidate_proof_v5 {
            crate::actions_write_journal::ActionsWriteJournal::open(database)
                .and_then(|journal| journal.validate_prepared_force_v5(proof))
                .map_err(|_| refuse("Original accepted Cancel changed before final principal"))?;
        }
        Ok(())
    })?;
    scope(&intent.source, closed.identity())?;
    retained = retained.saturating_sub(identity_bytes(&identity));
    identity = closed.identity().clone();
    retained = retained.saturating_add(identity_bytes(&identity));
    if retained > 8 * 1024 * 1024 {
        return Err(refuse("Final Cancel identity presentation exceeds8MiB"));
    }
    let value = Arc::new(OpenedActionsComposerV3 {
        intent,
        local,
        target,
        identity,
        lines,
        display,
        retained,
        history,
        force_candidate_v5,
        form_seed: std::sync::Mutex::new(Some(form_seed)),
    });
    clock(cancel, deadline)?;
    Ok(value)
}
pub(crate) fn preview(
    opened: Arc<OpenedActionsComposerV3>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<Arc<ScopedActionsPreviewV3>> {
    preview_with_options_v4(
        opened,
        ScopedRunOptionsV4::default(),
        provider,
        database,
        cancel,
        deadline,
    )
}
pub(crate) fn preview_with_options_v4(
    opened: Arc<OpenedActionsComposerV3>,
    options: ScopedRunOptionsV4,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<Arc<ScopedActionsPreviewV3>> {
    clock(cancel, deadline)?;
    let intent = &opened.intent;
    if intent.command() == ActionsCommand::ForceCancel && !options.force_acknowledged {
        return Err(refuse("Choose force_ack=true explicitly before Preview; normal Cancel is safer and existing barriers remain"));
    }
    opened.local.validate(database, &intent.source)?;
    let phase = provider.clone().with_catalogue_read_deadline(deadline);
    let mut session = phase.begin_repository_catalogue_access(
        intent.source.original().source(),
        &access_limits(),
        cancel,
    )?;
    scope(&intent.source, session.identity())?;
    let window = session.reserve_measured_external_reads(15, 512 * 1024, 1024 * 1024, 2, cancel)?;
    let measured = phase.clone().with_measured_read_window(window)?;
    session.verify(intent.source.reference(), cancel)?;
    let current = session
        .observed_original_v1(intent.source.reference().id)
        .ok_or_else(|| refuse("Original repository unavailable"))?;
    if let Some(pr) = intent.source.pr_origin_v2() {
        pr.fresh_parent(&measured, session.identity(), current, cancel)?;
    }
    let target = measured.scoped_actions_run_snapshot_v3(current, &opened.target, cancel)?;
    let mut workspace = tiny_workspace(session.identity(), &target)?;
    if let Some(job) = intent.selected_job() {
        if !job.run().same_watch_pin_v2(&target) {
            return Err(refuse("Actual selected job/run changed"));
        }
        workspace
            .run
            .as_mut()
            .expect("actual tiny run")
            .selected_job = Some(job.id());
    }
    let plan = ActionsPlan::prepare(
        &workspace,
        intent.command(),
        &options.values(intent.command()),
        &measured,
        cancel,
    )?;
    drop(workspace);
    let descriptor = plan.scoped_run_descriptor_v4(
        &measured,
        &intent.source,
        &target,
        intent.selected_job(),
        intent.action(options),
    )?;
    let force_proof_v5 = if intent.command() == ActionsCommand::ForceCancel {
        let journal = crate::actions_write_journal::ActionsWriteJournal::open(database)
            .map_err(|_| refuse("Original escalation journal unavailable"))?;
        let proof = journal
            .prepare_force_escalation_v5(
                &measured,
                &intent.source,
                &target,
                &descriptor.target,
                &descriptor.intent,
            )
            .map_err(|_| {
                refuse("Original accepted Cancel changed; inspect before another escalation")
            })?;
        if proof.as_ref().map(|p| p.metadata().cancel).as_ref()
            != opened.force_candidate_v5.as_ref()
        {
            return Err(refuse(
                "The displayed original Cancel changed; no new Force approval",
            ));
        }
        proof
    } else {
        None
    };
    let plan_bytes = plan.scoped_retained_bytes_v3()?;
    let display = PreparedFormTextV1::prepare(&plan.preview, cancel, deadline)
        .map_err(|_| refuse("Original Cancel Preview exceeds shared presentation bounds"))?;
    let retained = std::mem::size_of::<ScopedActionsPreviewV3>()
        + 4 * std::mem::size_of::<usize>()
        + opened.retained
        + target.owned_bytes_v1()
        + plan_bytes
        + display.retained_bytes();
    let peak = retained
        + plan_bytes
        + display.preparation_peak_bound()
        + if force_proof_v5.is_some() {
            512 * 1024
        } else {
            0
        };
    if plan_bytes > 8 * 1024 * 1024
        || display.retained_bytes() + opened.display.retained_bytes() > 16 * 1024 * 1024
        || retained > 24 * 1024 * 1024
        || peak > 16 * 1024 * 1024
    {
        return Err(refuse(
            "Original Cancel Plan/layout exceeds8/16/32MiB preparation bounds",
        ));
    }
    let closing = measured.scoped_actions_run_snapshot_v3(
        session
            .observed_original_v1(intent.source.reference().id)
            .ok_or_else(|| refuse("Original repository unavailable"))?,
        &target,
        cancel,
    )?;
    if !same_run(&closing, &target) {
        return Err(refuse(
            "Original run changed while preparing Preview; no approval",
        ));
    }
    drop(closing);
    if let Some(pr) = intent.source.pr_origin_v2() {
        pr.fresh_parent(
            &measured,
            session.identity(),
            session
                .observed_original_v1(intent.source.reference().id)
                .ok_or_else(|| refuse("Original repository unavailable"))?,
            cancel,
        )?;
    }
    session.verify(intent.source.reference(), cancel)?;
    let closed = session.close_with_guard(cancel, || {
        opened.local.validate(database, &intent.source)?;
        if let Some(proof) = &force_proof_v5 {
            crate::actions_write_journal::ActionsWriteJournal::open(database)
                .and_then(|journal| journal.validate_prepared_force_v5(proof))
                .map_err(|_| {
                    refuse("Displayed original Cancel changed before Preview principal")
                })?;
        }
        Ok(())
    })?;
    scope(&intent.source, closed.identity())?;
    let value = Arc::new(ScopedActionsPreviewV3 {
        opened,
        target,
        plan: Arc::new(plan),
        display,
        attempted: AtomicBool::new(false),
        options,
    });
    clock(cancel, deadline)?;
    Ok(value)
}
