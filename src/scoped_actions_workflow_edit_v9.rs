//! Exact retained workflow Definition and private local-edit input.
//! Neither a name-only local mapping nor this display intent grants a write.
use crate::{
    actions_workspace::ActionsView,
    providers::gh::{ScopedActionsWorkflowV2, WorkflowDefinition},
    scoped_actions_v1::{ScopedActionsPageV1, ScopedActionsSourceV1},
};
use std::{path::Path, sync::Arc, time::Instant};
mod budget_v9;
mod core_inputs_v9;
pub(crate) use core_inputs_v9::WorkflowLocalCoreInputsV9;
mod commit_v9;
use budget_v9::WorkflowUiBudgetV9;
pub(crate) mod draft_cutoff_v9;
pub(crate) mod draft_v9;
mod isolation_confirm_v9;
mod isolation_v9;
mod plan_v9;
mod remote_confirm_v9;
mod remote_preview_v9;
mod remote_v9;
pub(crate) use remote_confirm_v9::{
    PreparedWorkflowBranchV9, WorkflowPublishedBranchV9, WorkflowRemoteConfirmV9,
    WorkflowRemoteReleaseV9,
};
pub(crate) use remote_preview_v9::WorkflowRemotePreviewV9;
pub(crate) use remote_v9::{WorkflowHeadMemberV9, WorkflowRemoteSelectionV9};
mod save_v9;
mod stage_v9;
pub(crate) use commit_v9::{
    WorkflowCommitConfirmV9, WorkflowCommitOutcomeV9, WorkflowCommitReleaseV9,
    WorkflowCommittedLocalV9,
};
pub(crate) use isolation_confirm_v9::{
    WorkflowIsolatedLocalV9, WorkflowWorktreeConfirmV9, WorkflowWorktreeOutcomeV9,
    WorkflowWorktreeReleaseV9,
};
pub(crate) use isolation_v9::WorkflowWorktreePreviewV9;
pub(crate) use plan_v9::WorkflowLocalPlanPreviewV9;
pub(crate) use save_v9::{
    WorkflowSaveConfirmV9, WorkflowSaveOutcomeV9, WorkflowSavePreviewV9, WorkflowSaveReleaseV9,
    WorkflowSavedLocalV9,
};
pub(crate) use stage_v9::{
    WorkflowStageConfirmV9, WorkflowStageOutcomeV9, WorkflowStageReleaseV9, WorkflowStagedLocalV9,
};

pub(crate) const YAML_BYTES_V9: usize = 256 * 1024;
pub(crate) const COMMIT_MESSAGE_BYTES_V9: usize = 32 * 1024;
pub(crate) const PR_BODY_BYTES_V9: usize = 64 * 1024;
const INPUT_BYTES_V9: usize = 360 * 1024;

pub(crate) struct WorkflowEditIntentV9 {
    source: Arc<ScopedActionsSourceV1>,
    page: Arc<ScopedActionsPageV1>,
    original: Arc<ScopedActionsWorkflowV2>,
    definition: Arc<WorkflowDefinition>,
    epoch: u64,
    key: Arc<str>,
}
impl std::fmt::Debug for WorkflowEditIntentV9 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkflowEditIntentV9(<actual retained Definition; no association/approval>)")
    }
}
impl WorkflowEditIntentV9 {
    pub(crate) fn capture(page: Arc<ScopedActionsPageV1>, epoch: u64) -> Result<Arc<Self>, String> {
        let (original, definition) = page
            .resource_v2()
            .and_then(|resource| resource.workflow_definition_v6())
            .ok_or("Inspect an actual original workflow Definition before local editing")?;
        // Bound borrowed data BEFORE any input/source text is copied. The
        // original existing proof graph is retained by Arc, not deep-cloned.
        if page.view() != ActionsView::Workflow
            || original.repository().id() != page.source().reference().id
            || original.repository().node().is_none()
            || original.node_v6().is_none()
            || definition.workflow.id != original.model().id
            || definition.workflow.path != original.model().path
            || definition.workflow.state != original.model().state
            || !crate::git::original_actor_saved_sha(&definition.commit_sha)
            || definition.revision.is_empty()
            || definition.revision.len() > 4096
            || definition.revision.chars().any(char::is_control)
            || !safe_workflow_path_v9(&definition.workflow.path)
            || definition.yaml.is_empty()
            || definition.yaml.len() > YAML_BYTES_V9
            || definition.yaml.contains('\0')
        {
            return Err("Original workflow/path/revision is unavailable or exceeds the local editor bound; no default/local source selected".into());
        }
        let key: Arc<str> = format!(
            "scoped-actions-v9/repository/{}/workflow/{}/local-edit",
            original.repository().id(),
            original.model().id
        )
        .into();
        Ok(Arc::new(Self {
            source: page.source().clone(),
            original: original.clone(),
            definition: definition.clone(),
            page,
            epoch,
            key,
        }))
    }
    pub(crate) fn source(&self) -> &Arc<ScopedActionsSourceV1> {
        &self.source
    }
    pub(crate) fn page(&self) -> &Arc<ScopedActionsPageV1> {
        &self.page
    }
    pub(crate) fn original(&self) -> &Arc<ScopedActionsWorkflowV2> {
        &self.original
    }
    pub(crate) fn definition(&self) -> &Arc<WorkflowDefinition> {
        &self.definition
    }
    pub(crate) fn epoch(&self) -> u64 {
        self.epoch
    }
    pub(crate) fn key(&self) -> &str {
        &self.key
    }
}

/// Local draft content only. No serde/Debug body, provider, saved target,
/// confirmation, source factory or remote permission is carried by inputs.
pub(crate) struct WorkflowEditValuesV9 {
    yaml: Arc<str>,
    branch: Arc<str>,
    commit_message: Arc<str>,
    pr_title: Arc<str>,
    pr_body: Arc<str>,
}
impl std::fmt::Debug for WorkflowEditValuesV9 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkflowEditValuesV9(<all YAML/message/PR text redacted>)")
    }
}
impl WorkflowEditValuesV9 {
    pub(crate) fn capture(
        yaml: &str,
        branch: &str,
        commit_message: &str,
        pr_title: &str,
        pr_body: &str,
    ) -> Result<Arc<Self>, String> {
        // The SAME YAML grammar/expansion preflight is used only on the
        // worker for Preview. This pure input gate bounds before allocation;
        // it never does parsing/disk/SQL/process work on an input event.
        let bytes = [yaml, branch, commit_message, pr_title, pr_body]
            .iter()
            .try_fold(0usize, |sum, value| sum.checked_add(value.len()))
            .ok_or("Workflow input byte accounting overflow; no input copied")?;
        if bytes > INPUT_BYTES_V9
            || yaml.is_empty()
            || yaml.len() > YAML_BYTES_V9
            || yaml.contains('\0')
            || !safe_new_branch_v9(branch)
            || commit_message.trim().is_empty()
            || commit_message.len() > COMMIT_MESSAGE_BYTES_V9
            || commit_message.contains('\0')
            || pr_title.trim().is_empty()
            || pr_title.len() > 1024
            || pr_title.chars().any(char::is_control)
            || pr_body.len() > PR_BODY_BYTES_V9
            || pr_body.contains('\0')
        {
            return Err("Workflow YAML/branch/commit/PR input is incomplete or exceeds its fixed private bound; no operation queued".into());
        }
        Ok(Arc::new(Self {
            yaml: yaml.into(),
            branch: branch.into(),
            commit_message: commit_message.into(),
            pr_title: pr_title.into(),
            pr_body: pr_body.into(),
        }))
    }
    pub(crate) fn validate_yaml_worker(&self) -> Result<(), String> {
        WorkflowDefinition::validate_edit_yaml_v9(&self.yaml).map_err(|failure| failure.message)
    }
    pub(crate) fn yaml(&self) -> &str {
        &self.yaml
    }
    pub(crate) fn branch(&self) -> &str {
        &self.branch
    }
    pub(crate) fn commit_message(&self) -> &str {
        &self.commit_message
    }
    pub(crate) fn pr_title(&self) -> &str {
        &self.pr_title
    }
    pub(crate) fn pr_body(&self) -> &str {
        &self.pr_body
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + 2 * std::mem::size_of::<usize>()
            + [
                &self.yaml,
                &self.branch,
                &self.commit_message,
                &self.pr_title,
                &self.pr_body,
            ]
            .iter()
            .map(|value| value.len() + 2 * std::mem::size_of::<usize>())
            .sum::<usize>()
    }
}
pub(crate) fn safe_workflow_path_v9(path: &str) -> bool {
    path.len() <= 4096
        && path.starts_with(".github/workflows/")
        && path.len() > ".github/workflows/".len()
        && !path.chars().any(char::is_control)
        && !path.contains('\\')
        && path.split('/').all(|part| !matches!(part, "" | "." | ".."))
}
/// Pure bounded spelling check only. The worker ALSO runs installed Git's
/// exact check-ref-format for the chosen branch before any write permit.
fn safe_new_branch_v9(branch: &str) -> bool {
    !branch.is_empty()
        && branch.len() <= 256
        && !branch.starts_with('-')
        && !branch.starts_with("refs/")
        && !branch.ends_with('.')
        && !branch.contains("..")
        && !branch.contains("@{")
        && branch != "@"
        && !branch.chars().any(|ch| {
            ch.is_control()
                || ch.is_whitespace()
                || matches!(ch, '~' | '^' | ':' | '?' | '*' | '[' | '\\')
        })
        && branch
            .split('/')
            .all(|part| !part.is_empty() && !part.starts_with('.') && !part.ends_with(".lock"))
}

pub(crate) struct OpenedWorkflowLocalEditV9 {
    intent: Arc<WorkflowEditIntentV9>,
    binding: Arc<crate::git::workflow_edit_v9::WorkflowLocalBindingV9>,
    local: Arc<crate::scoped_actions_publication_v3::LocalV3>,
    identity: crate::providers::gh::HostIdentity,
    lines: Vec<String>,
    display: Arc<crate::prepared_form_text_v1::PreparedFormTextV1>,
    retained: usize,
}
impl std::fmt::Debug for OpenedWorkflowLocalEditV9 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OpenedWorkflowLocalEditV9(<closed matching-byte observation; no approval>)")
    }
}
impl OpenedWorkflowLocalEditV9 {
    pub(crate) fn intent(&self) -> &Arc<WorkflowEditIntentV9> {
        &self.intent
    }
    pub(crate) fn binding(&self) -> &Arc<crate::git::workflow_edit_v9::WorkflowLocalBindingV9> {
        &self.binding
    }
    pub(crate) fn identity(&self) -> &crate::providers::gh::HostIdentity {
        &self.identity
    }
    pub(crate) fn lines(&self) -> &[String] {
        &self.lines
    }
    pub(crate) fn display(&self) -> &Arc<crate::prepared_form_text_v1::PreparedFormTextV1> {
        &self.display
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        self.retained
    }
    pub(crate) fn validate_local(
        &self,
        database: &Path,
        cancel: &crate::providers::runner::CancellationToken,
    ) -> crate::providers::gh::GithubResult<()> {
        self.local.validate(database, self.intent.source())?;
        let cache = crate::storage::Cache::open(database).map_err(|_| local_edit_refused_v9())?;
        self.binding
            .validate_local(&cache, database, cancel)
            .map_err(|_| local_edit_refused_v9())
    }
    pub(crate) fn validate_owned_remote_v9(
        &self,
        database: &Path,
        cancel: &crate::providers::runner::CancellationToken,
        journal: &crate::actions_write_journal::ActionsWriteJournal,
        claim: &crate::actions_write_journal::ActionsClaimStampV3,
        result: Option<(
            crate::actions_write_journal::ActionsWriteState,
            &crate::actions_write_journal::ActionsReceipt,
        )>,
    ) -> crate::providers::gh::GithubResult<()> {
        self.local
            .validate_owned_v6(database, self.intent.source(), journal, claim, result)?;
        let cache = crate::storage::Cache::open(database).map_err(|_| local_edit_refused_v9())?;
        self.binding
            .validate_owned_local_v9(&cache, database, cancel, journal, claim, result)
            .map_err(|_| local_edit_refused_v9())
    }
    pub(crate) fn validate_after_own_branch_v9(
        &self,
        database: &Path,
        cancel: &crate::providers::runner::CancellationToken,
        branch: &WorkflowPublishedBranchV9,
    ) -> crate::providers::gh::GithubResult<()> {
        if !branch.binds_opened_v9(self) {
            return Err(local_edit_refused_v9());
        }
        let journal = crate::actions_write_journal::ActionsWriteJournal::open(database)
            .map_err(|_| local_edit_refused_v9())?;
        self.local.validate_workflow_branch_transition_v9(
            database,
            self.intent.source(),
            &journal,
            branch,
        )?;
        let cache = crate::storage::Cache::open(database).map_err(|_| local_edit_refused_v9())?;
        self.binding
            .validate_after_own_branch_v9(&cache, database, cancel, &journal, branch)
            .map_err(|_| local_edit_refused_v9())
    }
}
fn local_edit_refused_v9() -> crate::providers::gh::GithubError {
    crate::providers::gh::GithubError { kind: crate::providers::gh::GithubErrorKind::IdentityChanged,
        message: "Original workflow/local mapping/bytes/account unavailable; no local or remote write and no fallback".into() }
}
// Bounds only: the actual measured/cursor deadline may be STRICTLY narrower
// than the explicit queued request. It can never replace/extend that ceiling.
fn effective_phase_deadline_v9(
    cancel: &crate::providers::runner::CancellationToken,
    requested: Instant,
    actual: Instant,
) -> crate::providers::gh::GithubResult<Instant> {
    crate::scoped_actions_publication_v3::clock(cancel, requested)?;
    crate::scoped_actions_publication_v3::clock(cancel, actual)?;
    if actual > requested {
        return Err(local_edit_refused_v9());
    }
    Ok(actual)
}
fn identity_bytes_v9(identity: &crate::providers::gh::HostIdentity) -> usize {
    identity.host.capacity() + identity.login.capacity() + identity.gh_version.capacity()
}
/// Worker only. One original request clock/Core/pool, actual source SQL after
/// initial remote access, genuine mapping and matching C/path bytes. No local
/// mutation, source activation, approval, default mapping or new queue.
#[cfg(test)]
pub(crate) fn prepare_local_open_v9(
    intent: Arc<WorkflowEditIntentV9>,
    provider: &crate::providers::gh::GithubProvider,
    database: &Path,
    cancel: &crate::providers::runner::CancellationToken,
    deadline: Instant,
) -> crate::providers::gh::GithubResult<Arc<OpenedWorkflowLocalEditV9>> {
    prepare_local_open_seed_v9(
        intent,
        provider,
        database,
        cancel,
        deadline,
        false,
        WorkflowUiBudgetV9::standalone(),
    )
    .map(|(opened, _)| opened)
}
#[cfg(test)]
pub(crate) fn prepare_local_open_native_v9(
    intent: Arc<WorkflowEditIntentV9>,
    provider: &crate::providers::gh::GithubProvider,
    database: &Path,
    cancel: &crate::providers::runner::CancellationToken,
    deadline: Instant,
) -> crate::providers::gh::GithubResult<(
    Arc<OpenedWorkflowLocalEditV9>,
    crate::scoped_actions_workflow_edit_ui_v9::WorkflowEditFormSeedV9,
)> {
    let (opened, seed) = prepare_local_open_seed_v9(
        intent,
        provider,
        database,
        cancel,
        deadline,
        true,
        WorkflowUiBudgetV9::standalone(),
    )?;
    Ok((opened, seed.ok_or_else(local_edit_refused_v9)?))
}
/// Native product entry requires its actual borrowed App/queue holder. The
/// standalone no-App wrappers below are not a fallback from a failed capture.
pub(crate) fn prepare_local_held_native_v9(
    intent: Arc<WorkflowEditIntentV9>,
    old: Option<&crate::scoped_actions_workflow_edit_ui_v9::ClosedWorkflowFormV9>,
    presentation:&crate::scoped_actions_native_v1::publication_v3::workflow_edit_v9::WorkflowNativePresentationV9,
    provider: &crate::providers::gh::GithubProvider,
    database: &Path,
    cancel: &crate::providers::runner::CancellationToken,
    deadline: Instant,
) -> crate::providers::gh::GithubResult<(
    Arc<OpenedWorkflowLocalEditV9>,
    crate::scoped_actions_workflow_edit_ui_v9::WorkflowEditFormSeedV9,
)> {
    let budget = WorkflowUiBudgetV9::for_live_native_open_v9(&intent, old, presentation)?;
    let (opened, seed) =
        prepare_local_open_seed_v9(intent, provider, database, cancel, deadline, true, budget)?;
    Ok((opened, seed.ok_or_else(local_edit_refused_v9)?))
}
/// The native reopening path must debit the genuine still-retained dismissed
/// presentation. No caller supplies a numeric allowance or reconstructs it.
#[cfg(test)]
pub(crate) fn prepare_local_reopen_native_v9(
    intent: Arc<WorkflowEditIntentV9>,
    old: &crate::scoped_actions_workflow_edit_ui_v9::ClosedWorkflowFormV9,
    provider: &crate::providers::gh::GithubProvider,
    database: &Path,
    cancel: &crate::providers::runner::CancellationToken,
    deadline: Instant,
) -> crate::providers::gh::GithubResult<(
    Arc<OpenedWorkflowLocalEditV9>,
    crate::scoped_actions_workflow_edit_ui_v9::WorkflowEditFormSeedV9,
)> {
    if !old.matches_intent(&intent) {
        return Err(local_edit_refused_v9());
    }
    let budget = WorkflowUiBudgetV9::for_reopen(old)?;
    let (opened, seed) =
        prepare_local_open_seed_v9(intent, provider, database, cancel, deadline, true, budget)?;
    Ok((opened, seed.ok_or_else(local_edit_refused_v9)?))
}
fn prepare_local_open_seed_v9(
    intent: Arc<WorkflowEditIntentV9>,
    provider: &crate::providers::gh::GithubProvider,
    database: &Path,
    cancel: &crate::providers::runner::CancellationToken,
    deadline: Instant,
    native: bool,
    budget: WorkflowUiBudgetV9,
) -> crate::providers::gh::GithubResult<(
    Arc<OpenedWorkflowLocalEditV9>,
    Option<crate::scoped_actions_workflow_edit_ui_v9::WorkflowEditFormSeedV9>,
)> {
    use crate::{providers::gh::*, scoped_actions_publication_v3 as shared, storage::Cache};
    shared::clock(cancel, deadline)?;
    budget.before_native_prepare(0)?;
    if deadline.saturating_duration_since(Instant::now()) > std::time::Duration::from_secs(30) {
        return Err(local_edit_refused_v9());
    }
    let core_inputs = budget.core_open_v9(&intent)?;
    let core_policy = WorkflowCoreJsonV9::for_local_v9(provider, &core_inputs, cancel, deadline)?;
    let phase = provider.clone().with_catalogue_read_deadline(deadline);
    let mut session = phase.begin_workflow_edit_access_v9(
        intent.source().original().source(),
        &shared::access_limits(),
        &core_policy,
        cancel,
    )?;
    shared::scope(intent.source(), session.identity())?;
    session.verify(intent.source().reference(), cancel)?;
    // First private source/mapping reads ONLY after actual principal/repo.
    let local = Arc::new(shared::LocalV3::capture(database, intent.source())?);
    let cache = Cache::open(database).map_err(|_| local_edit_refused_v9())?;
    let mapping = Arc::new(
        cache
            .remote_mapping_candidate_v1(
                intent.source().original().scope(),
                &crate::search::managed::mapping_key(&intent.source().reference().last_name),
            )
            .map_err(|_| local_edit_refused_v9())?
            .ok_or_else(local_edit_refused_v9)?,
    );
    drop(cache);
    let (window, post) = session.reserve_workflow_edit_transport_v9(
        intent.original(),
        WorkflowEditPhaseV9::Bind,
        1,
        cancel,
    )?;
    if post.is_some() {
        return Err(local_edit_refused_v9());
    }
    let effective = effective_phase_deadline_v9(cancel, deadline, window.deadline())?;
    let measured = phase.clone().with_measured_read_window(window)?;
    let heap = core_policy.bind_measured_v9(&measured, cancel)?;
    let current = session
        .observed_original_v1(intent.source().reference().id)
        .ok_or_else(local_edit_refused_v9)?;
    let canonical = current.repository().full_name.clone();
    if let Some(pr) = intent.source().pr_origin_v2() {
        pr.fresh_workflow_edit_parent_v9(&measured, session.identity(), current, &heap, cancel)?;
    }
    let definition_read: WorkflowDefinitionReadV9 = measured
        .open_workflow_definition_edit_with_heap_v9(
            current,
            intent.original(),
            intent.definition(),
            session.identity(),
            &heap,
            cancel,
        )?;
    let binding = crate::git::workflow_edit_v9::WorkflowLocalBindingV9::read(
        intent.clone(),
        mapping,
        measured.remote_git_read_window_v1(cancel)?,
        cancel,
    )
    .map_err(|_| local_edit_refused_v9())?;
    let lines = vec![
        "Edit actual workflow locally • matching-byte observation ONLY".into(),
        format!("Repository {} • numeric ID {} • workflow {}", canonical, intent.source().reference().id, intent.original().model().id),
        format!("Selected workflow ref: {}", intent.definition().revision),
        format!("Exact saved definition commit C: {}", intent.definition().commit_sha),
        format!("Exact path: {}", intent.definition().workflow.path),
        format!("Actual matching local candidate: {}", binding.root_label()),
        format!("Observed regular mode {} • blob {}", binding.mode(), binding.blob()),
        "A saved name-only mapping is NOT remote association or write permission.".into(),
        "Isolated worktree/branch, Save, Stage, Commit, create-only Push and PR each require their own visible Preview and explicit Confirm.".into(),
        "Opening/editing is read-only; existing working files/index/config and remote branches stay unchanged.".into(),
        "Git hooks/signing remain enabled. No automatic stash, reset, force replacement, retry, cleanup or credential setup.".into(),
    ];
    let mut identity = session.identity().clone();
    let retained_before_display = std::mem::size_of::<OpenedWorkflowLocalEditV9>()
        + 2 * std::mem::size_of::<usize>()
        + std::mem::size_of::<WorkflowEditIntentV9>()
        + 2 * std::mem::size_of::<usize>()
        + intent.key().len()
        + binding.retained_bytes()
        + local.owned_bytes_v6()
        + 2 * std::mem::size_of::<usize>()
        + identity_bytes_v9(&identity)
        + lines.capacity() * std::mem::size_of::<String>()
        + lines.iter().map(String::capacity).sum::<usize>();
    let display = crate::prepared_form_text_v1::PreparedFormTextV1::prepare_with_remaining_v9(
        &lines,
        cancel,
        effective,
        budget.retained_remaining(retained_before_display)?,
        budget.peak_remaining(retained_before_display)?,
    )
    .map_err(|_| local_edit_refused_v9())?;
    let mut retained = retained_before_display + display.retained_bytes();
    budget.check_retained(retained)?;
    budget.check_peak(
        retained
            .checked_add(display.preparation_peak_bound())
            .ok_or_else(local_edit_refused_v9)?,
    )?;
    let seed = if native {
        // Worst-case encoded raw + editor/body allocations are reserved BEFORE
        // SQL can copy a private draft. Old wrapper performs no extra SQL.
        let temporary =
            (2 * draft_v9::WORKFLOW_DRAFT_BYTES_V9 + 2 * INPUT_BYTES_V9 + 64 * 1024usize)
                .checked_add(
                    crate::scoped_actions_workflow_edit_ui_v9::remote_fields_preparation_bound_v9(
                        &intent,
                    )
                    .map_err(|_| local_edit_refused_v9())?,
                )
                .ok_or_else(local_edit_refused_v9)?;
        budget.check_retained(
            retained
                .checked_add(temporary)
                .ok_or_else(local_edit_refused_v9)?,
        )?;
        let raw = Cache::open(database)
            .map_err(|_| local_edit_refused_v9())?
            .bounded_workflow_edit_draft_v9(&intent)
            .map_err(|_| local_edit_refused_v9())?;
        let seed = crate::scoped_actions_workflow_edit_ui_v9::WorkflowEditFormSeedV9::prepare(
            intent.clone(),
            &identity,
            display.clone(),
            raw.as_deref(),
        )
        .map_err(|_| local_edit_refused_v9())?;
        let raw_capacity = raw.as_ref().map_or(0, String::capacity);
        budget.check_retained(
            retained
                .checked_add(seed.retained_bytes())
                .and_then(|n| n.checked_add(raw_capacity))
                .ok_or_else(local_edit_refused_v9)?,
        )?;
        retained = retained
            .checked_add(seed.retained_bytes())
            .ok_or_else(local_edit_refused_v9)?;
        drop(raw); // Raw decoder material is gone BEFORE closing proof/UID.
        Some(seed)
    } else {
        None
    };
    definition_read.close_with_heap_v9(&measured, &heap, cancel)?;
    if let Some(pr) = intent.source().pr_origin_v2() {
        pr.fresh_workflow_edit_parent_v9(&measured, session.identity(), current, &heap, cancel)?;
    }
    let last = session.verify(intent.source().reference(), cancel)?;
    if last.repository().full_name != canonical {
        return Err(local_edit_refused_v9());
    }
    let closed = session.close_with_guard(cancel, || {
        local.validate(database, intent.source())?;
        let cache = Cache::open(database).map_err(|_| local_edit_refused_v9())?;
        binding
            .validate_local(&cache, database, cancel)
            .map_err(|_| local_edit_refused_v9())?;
        shared::clock(cancel, effective)
    })?;
    shared::scope(intent.source(), closed.identity())?;
    // O(1) actual capacity replacement only after final UID. No provider,
    // source SQL, YAML parse/clone, rows/fields/Doc construction occurs here.
    retained -= identity_bytes_v9(&identity);
    identity = closed.identity().clone();
    retained += identity_bytes_v9(&identity);
    budget.check_retained(retained)?;
    shared::clock(cancel, effective)?;
    Ok((
        Arc::new(OpenedWorkflowLocalEditV9 {
            intent,
            binding,
            local,
            identity,
            lines,
            display,
            retained,
        }),
        seed,
    ))
}
