//! Closed workflow state intent. Saved labels and Run targets cannot issue it.
//! Preview is display only; explicit Confirm owns one new original attempt.
use crate::{
    actions_workspace::{ActionsCommand, ActionsPlan, ActionsView, ActionsWorkspace},
    actions_write_journal::{
        ActionsClaimStampV3, ActionsReceipt, ActionsWriteJournal, ActionsWriteState,
    },
    prepared_form_text_v1::PreparedFormTextV1,
    providers::{gh::*, runner::CancellationToken},
    scoped_actions_publication_v3::{self as shared, LocalV3},
    scoped_actions_v1::{ScopedActionsPageV1, ScopedActionsSourceV1},
};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};
#[path = "scoped_actions_workflow_history_v6.rs"]
mod history_v6;
pub(crate) use history_v6::{inspect, WorkflowHistoryV6};
fn error(message: &'static str) -> GithubError {
    GithubError {
        kind: GithubErrorKind::IdentityChanged,
        message: message.into(),
    }
}
pub struct ScopedWorkflowIntentV6 {
    source: Arc<ScopedActionsSourceV1>,
    page: Arc<ScopedActionsPageV1>,
    original: Arc<ScopedActionsWorkflowV2>,
    definition: Arc<WorkflowDefinition>,
    enabled: bool,
    epoch: u64,
    key: Arc<str>,
}
impl std::fmt::Debug for ScopedWorkflowIntentV6 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedWorkflowIntentV6(<actual workflow, not run>)")
    }
}
impl ScopedWorkflowIntentV6 {
    pub(crate) fn capture(
        page: Arc<ScopedActionsPageV1>,
        epoch: u64,
        enabled: bool,
    ) -> Result<Arc<Self>, String> {
        let (original, definition) = page
            .resource_v2()
            .and_then(|resource| resource.workflow_definition_v6())
            .ok_or("Inspect an actual original workflow Definition first")?;
        if original.node_v6().is_none()
            || original.repository().node().is_none()
            || original.repository().id() != page.source().reference().id
            || definition.workflow.id != original.model().id
            || definition.workflow.path != original.model().path
            || definition.workflow.state != original.model().state
            || !state_eligible_v6(original.model(), enabled)
        {
            return Err("Workflow identity/state is unsupported or already desired; refresh, no write granted".into());
        }
        let key: Arc<str> = format!(
            "scoped-actions-v6/repository/{}/workflow/{}/{}",
            original.repository().id(),
            original.model().id,
            if enabled { "enable" } else { "disable" }
        )
        .into();
        if key.len() > 1024 {
            return Err("Workflow draft key exceeds1KiB".into());
        }
        Ok(Arc::new(Self {
            source: page.source().clone(),
            original: original.clone(),
            definition: definition.clone(),
            page,
            epoch,
            enabled,
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
    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }
    pub(crate) fn epoch(&self) -> u64 {
        self.epoch
    }
    pub(crate) fn key(&self) -> &str {
        &self.key
    }
    pub(crate) fn command(&self) -> ActionsCommand {
        if self.enabled {
            ActionsCommand::EnableWorkflow
        } else {
            ActionsCommand::DisableWorkflow
        }
    }
}
pub struct OpenedWorkflowV6 {
    intent: Arc<ScopedWorkflowIntentV6>,
    local: LocalV3,
    target: Arc<ScopedActionsWorkflowV2>,
    identity: HostIdentity,
    lines: Vec<String>,
    display: Arc<PreparedFormTextV1>,
    seed: Mutex<Option<crate::forms::Form>>,
    retained: usize,
}
impl std::fmt::Debug for OpenedWorkflowV6 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OpenedWorkflowV6(<closed display>)")
    }
}
impl OpenedWorkflowV6 {
    pub(crate) fn intent(&self) -> &Arc<ScopedWorkflowIntentV6> {
        &self.intent
    }
    pub(crate) fn target(&self) -> &Arc<ScopedActionsWorkflowV2> {
        &self.target
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
    pub(crate) fn retained(&self) -> usize {
        self.retained
    }
    pub(crate) fn take_seed(&self) -> Result<crate::forms::Form, String> {
        self.seed
            .lock()
            .map_err(|_| "Workflow Form seed unavailable")?
            .take()
            .ok_or_else(|| "Workflow Form seed consumed".into())
    }
    pub(crate) fn validate_local(&self, database: &Path) -> GithubResult<()> {
        self.local.validate(database, &self.intent.source)
    }
    pub(crate) fn validate_owned(
        &self,
        database: &Path,
        journal: &ActionsWriteJournal,
        stamp: &ActionsClaimStampV3,
        result: Option<(ActionsWriteState, &ActionsReceipt)>,
    ) -> GithubResult<()> {
        self.local
            .validate_owned_v6(database, &self.intent.source, journal, stamp, result)
    }
}
pub struct WorkflowPreviewV6 {
    opened: Arc<OpenedWorkflowV6>,
    target: Arc<ScopedActionsWorkflowV2>,
    plan: Arc<ActionsPlan>,
    display: Arc<PreparedFormTextV1>,
    attempted: AtomicBool,
}
impl std::fmt::Debug for WorkflowPreviewV6 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkflowPreviewV6(<display, no live API lease>)")
    }
}
impl WorkflowPreviewV6 {
    pub(crate) fn opened(&self) -> &Arc<OpenedWorkflowV6> {
        &self.opened
    }
    pub(crate) fn target(&self) -> &Arc<ScopedActionsWorkflowV2> {
        &self.target
    }
    pub(crate) fn plan(&self) -> &Arc<ActionsPlan> {
        &self.plan
    }
    pub(crate) fn display(&self) -> &Arc<PreparedFormTextV1> {
        &self.display
    }
    pub(crate) fn available(&self) -> bool {
        !self.attempted.load(Ordering::Acquire)
    }
}
pub struct WorkflowConfirmV6 {
    preview: Arc<WorkflowPreviewV6>,
    cancellation: CancellationToken,
    deadline: Instant,
    started: AtomicBool,
}
impl WorkflowConfirmV6 {
    pub(crate) fn capture(
        preview: Arc<WorkflowPreviewV6>,
        cancellation: &CancellationToken,
        deadline: Instant,
    ) -> Result<Arc<Self>, String> {
        shared::clock(cancellation, deadline).map_err(|_| "Original workflow Confirm ended")?;
        preview
            .attempted
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "Original workflow Preview already attempted")?;
        Ok(Arc::new(Self {
            preview,
            cancellation: cancellation.clone(),
            deadline,
            started: AtomicBool::new(false),
        }))
    }
    pub(crate) fn preview(&self) -> &Arc<WorkflowPreviewV6> {
        &self.preview
    }
    pub(crate) fn cancellation(&self) -> &CancellationToken {
        &self.cancellation
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }
    pub(crate) fn start(&self) -> GithubResult<()> {
        shared::clock(&self.cancellation, self.deadline)?;
        self.started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| ())
            .map_err(|_| error("Original workflow Confirm already attempted"))
    }
}
fn identity_bytes(identity: &HostIdentity) -> usize {
    identity.host.capacity() + identity.login.capacity() + identity.gh_version.capacity()
}
fn lines_bytes(lines: &Vec<String>) -> usize {
    lines.capacity() * std::mem::size_of::<String>()
        + lines.iter().map(String::capacity).sum::<usize>()
}
pub(crate) fn definition_clone_preflight_v6(definition: &WorkflowDefinition) -> GithubResult<()> {
    // This is the SAME canonical definition accounting as the resource viewer,
    // including vector/string capacities and bounded opaque JSON-node charges.
    // Check BEFORE cloning into Form/Plan; YAML length alone does not bound its
    // parsed matrix/jobs. Oversize definitions remain readable in Definition.
    if crate::actions_workspace::scoped_resources_v2::definition_bytes(definition) > 1024 * 1024 {
        return Err(error(
            "Workflow confirmation model exceeds1MiB; Definition remains read only",
        ));
    }
    Ok(())
}
pub(crate) fn display_v6(
    lines: &[String],
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<Arc<PreparedFormTextV1>> {
    // A workflow intent/preview has only bounded target/effect labels, not YAML.
    // Non-expanding shared sanitation and <=32768 raw bytes bound BOTH layout
    // row counts to <=32768+32 BEFORE the shared immutable Doc allocates them.
    if lines.len() > 32
        || lines
            .iter()
            .try_fold(0usize, |sum, line| sum.checked_add(line.len()))
            .is_none_or(|bytes| bytes > 32 * 1024)
    {
        return Err(error(
            "Workflow effect labels exceed32KiB/32lines; no partial confirmation",
        ));
    }
    PreparedFormTextV1::prepare(lines, cancel, deadline)
        .map_err(|_| error("Workflow display exceeds original shared bounds"))
}
pub(crate) fn tiny_workspace(
    identity: &HostIdentity,
    intent: &ScopedWorkflowIntentV6,
    target: &ScopedActionsWorkflowV2,
) -> GithubResult<ActionsWorkspace> {
    definition_clone_preflight_v6(intent.definition())?;
    if target.owned_bytes_v2() > 64 * 1024
        || intent.definition().yaml.len() > 512 * 1024
        || identity_bytes(identity) > 16 * 1024
    {
        return Err(error(
            "Workflow model exceeds original metadata/YAML bounds",
        ));
    }
    // Worker-only canonical clone. No foreground YAML clone or fake Run page.
    let mut definition = (**intent.definition()).clone();
    definition.workflow = target.model().clone();
    let mut workspace =
        ActionsWorkspace::new(identity.clone(), vec![target.repository().name().into()]);
    workspace.view = ActionsView::Workflow;
    workspace.definition = Some(definition);
    workspace.workflows = vec![target.model().clone()];
    Ok(workspace)
}
pub(crate) fn open(
    intent: Arc<ScopedWorkflowIntentV6>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<Arc<OpenedWorkflowV6>> {
    shared::clock(cancel, deadline)?;
    let local = LocalV3::capture(database, &intent.source)?;
    let phase = provider.clone().with_catalogue_read_deadline(deadline);
    let mut session = phase.begin_repository_catalogue_access(
        intent.source.original().source(),
        &shared::access_limits(),
        cancel,
    )?;
    shared::scope(&intent.source, session.identity())?;
    let window = session.reserve_measured_external_reads(15, 512 * 1024, 1024 * 1024, 2, cancel)?;
    let measured = phase.clone().with_measured_read_window(window)?;
    session.verify(intent.source.reference(), cancel)?;
    let current = session
        .observed_original_v1(intent.source.reference().id)
        .ok_or_else(|| error("Original workflow repository unavailable"))?;
    if let Some(pr) = intent.source.pr_origin_v2() {
        pr.fresh_parent(&measured, session.identity(), current, cancel)?;
    }
    let target =
        measured.scoped_actions_workflow_snapshot_v6(current, intent.original(), true, cancel)?;
    if !state_eligible_v6(target.model(), intent.enabled) {
        return Err(error("Workflow state no longer eligible"));
    }
    let workspace = tiny_workspace(session.identity(), &intent, &target)?;
    let mut seed_bytes = crate::actions_workspace::scoped_read_v1::retained_bytes(&workspace)
        .map_err(|_| error("Workflow seed exceeds shared bound"))?;
    let seed = crate::forms::Form::actions(workspace, intent.command())
        .map_err(|_| error("Actual workflow Form seed unavailable"))?;
    let crate::forms::FormPurpose::Actions(seed_intent) = &seed.purpose else {
        return Err(error("Actual workflow seed purpose differs"));
    };
    if !seed.fields.is_empty()
        || seed_intent.plan.is_some()
        || !seed.actions_original_lines.is_empty()
        || !seed.metadata_original_lines.is_empty()
        || seed.picker.is_some()
        || seed.metadata_preview.is_some()
        || seed.metadata_picker.is_some()
        || seed.base_selection.is_some()
    {
        return Err(error(
            "Unexpected workflow seed allocation; no partial intent",
        ));
    }
    // Actual seed-owned model, Box/Form headers, title/resource capacities and
    // both small presentation Arcs. Count remains conservative after take_seed.
    seed_bytes += std::mem::size_of::<crate::forms::Form>()
        + std::mem::size_of::<crate::forms::ActionsIntent>()
        + seed.title.capacity()
        + seed_intent.resource.capacity()
        + seed.fields.capacity() * std::mem::size_of::<crate::forms::Field>()
        + seed.field_rects.capacity() * std::mem::size_of::<ratatui::layout::Rect>()
        + 8 * std::mem::size_of::<usize>()
        + std::mem::size_of::<
            Option<crate::scoped_actions_workflow_ui_v6::WorkflowOutcomeFeedbackV25>,
        >();
    let mut lines=vec![format!("{} • entire repository • all refs",intent.command().label()),
        format!("Numeric repository {} • workflow {} • node {}",target.repository().id(),target.model().id,target.node_v6().unwrap_or("unavailable")),
        format!("Current repository: {} • original alias: {}",target.repository().name(),intent.source.reference().last_name),
        format!("Path: {}",target.model().path),format!("Observed state {} → requested {}",target.model().state,if intent.enabled {"active"}else{"disabled_manually"}),
        format!("Inspected definition ref {} • saved commit {}",intent.definition.revision,intent.definition.commit_sha),
        "Effect is repository-wide workflow state, NOT only this YAML, PR, commit, ref or run. Preview sends no PUT.".into(),
        "Only a NEW explicit Confirm may attempt one request after fresh source/principal/state checks. HTTP204 is acceptance; observed state is not causation.".into(),
        "After an attempt, Close keeps its protected record. Acknowledgement is local only, not proof of a remote effect.".into()];
    if let Some(pr) = intent.source.pr_origin_v2() {
        let p = pr.durable_pr_context_v3();
        lines.push(format!(
            "Original current PR #{} • head {} • base {}",
            p.number, p.head_sha, p.base_sha
        ));
    }
    let display = display_v6(&lines, cancel, deadline)?;
    let mut identity = session.identity().clone();
    let mut retained = std::mem::size_of::<OpenedWorkflowV6>()
        + 2 * std::mem::size_of::<usize>()
        + local.owned_bytes_v6()
        + std::mem::size_of::<ScopedWorkflowIntentV6>()
        + 2 * std::mem::size_of::<usize>()
        + intent.key.len()
        + target.owned_bytes_v2()
        + target.repository().owned_bytes_v2()
        + lines_bytes(&lines)
        + display.retained_bytes()
        + seed_bytes
        + identity_bytes(&identity);
    if retained > 8 * 1024 * 1024
        || retained.saturating_add(display.preparation_peak_bound()) > 16 * 1024 * 1024
    {
        return Err(error("Workflow preparation capacity exceeded"));
    }
    let closing = measured.scoped_actions_workflow_snapshot_v6(current, &target, true, cancel)?;
    if !closing.same_pin_v6(&target, true) {
        return Err(error("Workflow state changed during intent preparation"));
    }
    drop(closing);
    if let Some(pr) = intent.source.pr_origin_v2() {
        pr.fresh_parent(&measured, session.identity(), current, cancel)?;
    }
    session.verify(intent.source.reference(), cancel)?;
    let closed = session.close_with_guard(cancel, || {
        local.validate(database, &intent.source)?;
        shared::clock(cancel, deadline)
    })?;
    shared::scope(&intent.source, closed.identity())?;
    retained -= identity_bytes(&identity);
    identity = closed.identity().clone();
    retained += identity_bytes(&identity);
    if retained > 8 * 1024 * 1024 {
        return Err(error("Closed workflow presentation capacity exceeded"));
    }
    let result = Arc::new(OpenedWorkflowV6 {
        intent,
        local,
        target,
        identity,
        lines,
        display,
        seed: Mutex::new(Some(seed)),
        retained,
    });
    shared::clock(cancel, deadline)?;
    Ok(result)
}
pub(crate) fn prepare_preview(
    opened: Arc<OpenedWorkflowV6>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<Arc<WorkflowPreviewV6>> {
    shared::clock(cancel, deadline)?;
    opened.validate_local(database)?;
    let intent = &opened.intent;
    let phase = provider.clone().with_catalogue_read_deadline(deadline);
    let mut session = phase.begin_repository_catalogue_access(
        intent.source.original().source(),
        &shared::access_limits(),
        cancel,
    )?;
    shared::scope(&intent.source, session.identity())?;
    let window = session.reserve_measured_external_reads(15, 512 * 1024, 1024 * 1024, 2, cancel)?;
    let measured = phase.clone().with_measured_read_window(window)?;
    session.verify(intent.source.reference(), cancel)?;
    let current = session
        .observed_original_v1(intent.source.reference().id)
        .ok_or_else(|| error("Original workflow repository unavailable"))?;
    if let Some(pr) = intent.source.pr_origin_v2() {
        pr.fresh_parent(&measured, session.identity(), current, cancel)?;
    }
    let target =
        measured.scoped_actions_workflow_snapshot_v6(current, &opened.target, true, cancel)?;
    let workspace = tiny_workspace(session.identity(), intent, &target)?;
    let mut plan = ActionsPlan::prepare(
        &workspace,
        intent.command(),
        &Default::default(),
        &measured,
        cancel,
    )?;
    // The temporary seed and the actual cloned Plan overlap only here. Drop
    // the seed before preparing any immutable display/index material.
    drop(workspace);
    plan.scoped_workflow_state_descriptor_v6(&measured, &intent.source, &target, intent.enabled)?;
    // These are genuine effect labels, not authority. Definition bytes remain
    // the original actual source; canonical Plan remains the only operation.
    plan.preview.splice(0..0, opened.lines.iter().cloned());
    let display = display_v6(&plan.preview, cancel, deadline)?;
    let plan_bytes = plan.scoped_workflow_owned_bytes_v6()?;
    let retained = opened
        .retained
        .saturating_add(target.owned_bytes_v2())
        .saturating_add(plan_bytes)
        .saturating_add(display.retained_bytes());
    if retained > 8 * 1024 * 1024
        || retained.saturating_add(display.preparation_peak_bound()) > 16 * 1024 * 1024
    {
        return Err(error("Workflow Preview capacity exceeded"));
    }
    let closing = measured.scoped_actions_workflow_snapshot_v6(current, &target, true, cancel)?;
    if !closing.same_pin_v6(&target, true) {
        return Err(error("Workflow changed during Preview preparation"));
    }
    drop(closing);
    if let Some(pr) = intent.source.pr_origin_v2() {
        pr.fresh_parent(&measured, session.identity(), current, cancel)?;
    }
    session.verify(intent.source.reference(), cancel)?;
    let closed = session.close_with_guard(cancel, || {
        opened.validate_local(database)?;
        shared::clock(cancel, deadline)
    })?;
    shared::scope(&intent.source, closed.identity())?;
    let value = Arc::new(WorkflowPreviewV6 {
        opened,
        target,
        plan: Arc::new(plan),
        display,
        attempted: AtomicBool::new(false),
    });
    shared::clock(cancel, deadline)?;
    Ok(value)
}
