//! Closed original workflow Dispatch input/schema/Preview; no saved authority.
//! Provider work stays on the existing Actions queue and original phase clock.
use crate::{
    actions_workspace::{ActionsCommand, ActionsPlan, ActionsWorkspace},
    actions_write_journal::{
        ActionsClaimStampV3, ActionsReceipt, ActionsWriteJournal, ActionsWriteState,
    },
    forms::{Field, Form},
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
fn safe() -> GithubError {
    GithubError{kind:GithubErrorKind::IdentityChanged,message:"Original Dispatch source/schema/ref/principal unavailable • no retry/private diagnostics".into()}
}
pub(crate) struct ScopedDispatchIntentV7 {
    source: Arc<ScopedActionsSourceV1>,
    page: Arc<ScopedActionsPageV1>,
    original: Arc<ScopedActionsWorkflowV2>,
    epoch: u64,
    key: Arc<str>,
}
impl std::fmt::Debug for ScopedDispatchIntentV7 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedDispatchIntentV7(<actual original workflow>)")
    }
}
impl ScopedDispatchIntentV7 {
    pub(crate) fn capture(page: Arc<ScopedActionsPageV1>, epoch: u64) -> Result<Arc<Self>, String> {
        let (original, _) = page
            .resource_v2()
            .and_then(|resource| resource.workflow_definition_v6())
            .ok_or("Inspect an actual workflow Definition first")?;
        if original.node_v6().is_none()
            || original.repository().node().is_none()
            || original.repository().id() != page.source().reference().id
            || original.model().state != "active"
        {
            return Err(
                "Actual active original workflow required; no Dispatch permission inferred".into(),
            );
        }
        let key: Arc<str> = format!(
            "scoped-actions-v7/repository/{}/workflow/{}/dispatch",
            original.repository().id(),
            original.model().id
        )
        .into();
        Ok(Arc::new(Self {
            source: page.source().clone(),
            original: original.clone(),
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
    pub(crate) fn epoch(&self) -> u64 {
        self.epoch
    }
    pub(crate) fn key(&self) -> &str {
        &self.key
    }
}
pub(crate) struct OpenedDispatchV7 {
    intent: Arc<ScopedDispatchIntentV7>,
    local: LocalV3,
    identity: HostIdentity,
    lines: Vec<String>,
    display: Arc<PreparedFormTextV1>,
    seed: Mutex<Option<Form>>,
    retained: usize,
}
impl std::fmt::Debug for OpenedDispatchV7 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OpenedDispatchV7(<closed display>)")
    }
}
impl OpenedDispatchV7 {
    pub(crate) fn intent(&self) -> &Arc<ScopedDispatchIntentV7> {
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
    pub(crate) fn retained(&self) -> usize {
        self.retained
    }
    pub(crate) fn take_seed(&self) -> Result<Form, String> {
        self.seed
            .lock()
            .map_err(|_| "Dispatch seed unavailable")?
            .take()
            .ok_or_else(|| "Dispatch seed consumed".into())
    }
    pub(crate) fn validate_local(&self, database: &Path) -> GithubResult<()> {
        self.local.validate(database, self.intent.source())
    }
    pub(crate) fn validate_owned(
        &self,
        database: &Path,
        journal: &ActionsWriteJournal,
        claim: &ActionsClaimStampV3,
        result: Option<(ActionsWriteState, &ActionsReceipt)>,
    ) -> GithubResult<()> {
        self.local
            .validate_owned_v6(database, self.intent.source(), journal, claim, result)
    }
    pub(crate) fn validate_history(
        &self,
        database: &Path,
        journal: &ActionsWriteJournal,
        proof: &crate::actions_write_journal::ActionsOwnedDispatchHistoryV7,
    ) -> GithubResult<()> {
        self.local
            .validate_dispatch_history_v7(database, self.intent.source(), journal, proof)
    }
}
/// Privately worker-issued pairing. Text/layout is presentation only; neither
/// retaining it nor restoring it renews the source, schema clock or approval.
pub(crate) struct DispatchEditableSummaryV152 {
    schema: Arc<ScopedDispatchSchemaV7>,
    lines: Arc<[String]>,
    display: Arc<PreparedFormTextV1>,
}
impl DispatchEditableSummaryV152 {
    pub(crate) fn schema(&self) -> &Arc<ScopedDispatchSchemaV7> {
        &self.schema
    }
    pub(crate) fn lines(&self) -> &Arc<[String]> {
        &self.lines
    }
    pub(crate) fn display(&self) -> &Arc<PreparedFormTextV1> {
        &self.display
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + 6 * std::mem::size_of::<usize>()
            + self.schema.retained_bytes()
            + self.display.retained_bytes()
            + self.lines.len() * std::mem::size_of::<String>()
            + 2 * std::mem::size_of::<usize>()
            + self.lines.iter().map(String::capacity).sum::<usize>()
    }
}
pub(crate) struct PreparedDispatchSchemaV7 {
    opened: Arc<OpenedDispatchV7>,
    summary: Arc<DispatchEditableSummaryV152>,
    fields: Vec<Field>,
}
impl PreparedDispatchSchemaV7 {
    pub(crate) fn opened(&self) -> &Arc<OpenedDispatchV7> {
        &self.opened
    }
    pub(crate) fn schema(&self) -> &Arc<ScopedDispatchSchemaV7> {
        self.summary.schema()
    }
    pub(crate) fn lines(&self) -> &Arc<[String]> {
        self.summary.lines()
    }
    pub(crate) fn display(&self) -> &Arc<PreparedFormTextV1> {
        self.summary.display()
    }
    pub(crate) fn summary(&self) -> &Arc<DispatchEditableSummaryV152> {
        &self.summary
    }
    pub(crate) fn take_fields(self) -> Vec<Field> {
        self.fields
    }
}
pub(crate) struct DispatchPreviewV7 {
    opened: Arc<OpenedDispatchV7>,
    schema: Arc<ScopedDispatchSchemaV7>,
    inputs: Arc<CapturedDispatchInputsV7>,
    plan: Arc<ActionsPlan>,
    payload: Arc<[u8]>,
    display: Arc<PreparedFormTextV1>,
    attempted: AtomicBool,
    summary: Arc<DispatchEditableSummaryV152>,
}
impl std::fmt::Debug for DispatchPreviewV7 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DispatchPreviewV7(<display/input bytes redacted; no live lease>)")
    }
}
impl DispatchPreviewV7 {
    pub(crate) fn summary(&self) -> &Arc<DispatchEditableSummaryV152> {
        &self.summary
    }
    /// Only newly prepared Preview allocations; original summary/fields and
    /// opened source are charged separately while they remain simultaneously.
    pub(crate) fn new_material_bytes_v152(&self) -> GithubResult<usize> {
        Ok(std::mem::size_of::<Self>()
            + 12 * std::mem::size_of::<usize>()
            + self.schema.retained_bytes()
            + self.inputs.retained_bytes()
            + self.plan.scoped_dispatch_owned_bytes_v7()?
            + self.payload.len()
            + self.display.retained_bytes())
    }
    pub(crate) fn opened(&self) -> &Arc<OpenedDispatchV7> {
        &self.opened
    }
    pub(crate) fn schema(&self) -> &Arc<ScopedDispatchSchemaV7> {
        &self.schema
    }
    pub(crate) fn inputs(&self) -> &Arc<CapturedDispatchInputsV7> {
        &self.inputs
    }
    pub(crate) fn plan(&self) -> &Arc<ActionsPlan> {
        &self.plan
    }
    pub(crate) fn payload(&self) -> &Arc<[u8]> {
        &self.payload
    }
    pub(crate) fn display(&self) -> &Arc<PreparedFormTextV1> {
        &self.display
    }
    pub(crate) fn available(&self) -> bool {
        !self.attempted.load(Ordering::Acquire)
    }
}
pub(crate) struct DispatchConfirmV7 {
    preview: Arc<DispatchPreviewV7>,
    cancel: CancellationToken,
    deadline: Instant,
    started: AtomicBool,
}
impl DispatchConfirmV7 {
    pub(crate) fn capture(
        preview: Arc<DispatchPreviewV7>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Arc<Self>, String> {
        shared::clock(cancel, deadline).map_err(|_| "Original Dispatch attempt ended")?;
        preview
            .attempted
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "Exact Dispatch Preview already attempted")?;
        Ok(Arc::new(Self {
            preview,
            cancel: cancel.clone(),
            deadline,
            started: AtomicBool::new(false),
        }))
    }
    pub(crate) fn preview(&self) -> &Arc<DispatchPreviewV7> {
        &self.preview
    }
    pub(crate) fn cancellation(&self) -> &CancellationToken {
        &self.cancel
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }
    pub(crate) fn start(&self) -> GithubResult<()> {
        shared::clock(&self.cancel, self.deadline)?;
        self.started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| ())
            .map_err(|_| safe())
    }
}
fn identity_bytes(identity: &HostIdentity) -> usize {
    identity.host.capacity() + identity.login.capacity() + identity.gh_version.capacity()
}
fn line_bytes(lines: &Vec<String>) -> usize {
    lines.capacity() * std::mem::size_of::<String>()
        + lines.iter().map(String::capacity).sum::<usize>()
}
pub(crate) fn field_bytes(fields: &Vec<Field>) -> usize {
    fields.capacity() * std::mem::size_of::<Field>()
        + fields
            .iter()
            .map(|field| {
                field.key.capacity()
                    + field.label.capacity()
                    + field.editor.text.capacity()
                    + field.choices.capacity() * std::mem::size_of::<String>()
                    + field.choices.iter().map(String::capacity).sum::<usize>()
                    + field.choice_labels.capacity() * std::mem::size_of::<String>()
                    + field
                        .choice_labels
                        .iter()
                        .map(String::capacity)
                        .sum::<usize>()
            })
            .sum::<usize>()
}
fn capacity(retained: usize, transient: usize) -> GithubResult<()> {
    if retained > 8 * 1024 * 1024
        || retained
            .checked_add(transient)
            .is_none_or(|n| n > 16 * 1024 * 1024)
    {
        Err(safe())
    } else {
        Ok(())
    }
}
pub(crate) fn checked_capture_capacity_v152(retained: usize, extra: usize) -> GithubResult<()> {
    capacity(retained.checked_add(extra).ok_or_else(safe)?, 0)
}
fn display_preflight_v152(lines: &[String], retained: usize) -> GithubResult<()> {
    // SAME non-expanding sanitizer/wrapper, not another layout grammar.
    // Each output row consumes at least one input byte or an empty source
    // line. Two row arrays + one temporary array <=48 bytes per such unit;
    // safe Arc strings/headers and both pointer arrays fit the remaining80.
    // 4096 covers fixed headers/alignment. This is a conservative
    // pinned-layout allocation bound, not process heap/RSS introspection.
    let units = lines.iter().try_fold(lines.len(), |bytes, line| {
        bytes.checked_add(line.len()).ok_or_else(safe)
    })?;
    let bound = units
        .checked_mul(128)
        .and_then(|n| n.checked_add(4096))
        .ok_or_else(safe)?;
    capacity(retained.checked_add(bound).ok_or_else(safe)?, bound)
}
fn base_fields(
    choice: Option<&DispatchRefChoiceV7>,
    protocol: DispatchProtocolV7,
    default_branch: Option<&str>,
) -> GithubResult<Vec<Field>> {
    let make = |key: &str, label: &str, value: &str| {
        Field::text(key, label, value, false).map_err(|_| safe())
    };
    let mut namespace = make(
        "namespace",
        "Exact ref namespace",
        choice.map_or("branch", DispatchRefChoiceV7::label),
    )?;
    namespace.choices = vec!["branch".into(), "tag".into()];
    let name = choice
        .map(DispatchRefChoiceV7::name)
        .or(default_branch)
        .unwrap_or("");
    if name.len() > 1024 {
        return Err(safe());
    }
    let reference = make(
        "ref",
        "Literal short branch/tag name (not HEAD/fullref/object ID)",
        name,
    )?;
    let mut version = make(
        "protocol",
        "Host API protocol (must be positively supported)",
        protocol.version(),
    )?;
    version.choices = vec!["2022-11-28".into(), "2026-03-10".into()];
    Ok(vec![namespace, reference, version])
}
fn fields(schema: &Arc<ScopedDispatchSchemaV7>) -> GithubResult<Vec<Field>> {
    let mut fields = base_fields(Some(schema.choice()), schema.protocol(), None)?;
    for input in &schema.definition().dispatch_inputs {
        if input.name.len() > 256
            || input.options.len() > 256
            || input.options.iter().any(|v| v.len() > 1024)
        {
            return Err(safe());
        }
        let mut mode = Field::text(
            &format!("mode:{}", input.name),
            &format!(
                "{} • {}{} • mode",
                input.name,
                match input.input_type {
                    DispatchInputType::String => "text",
                    DispatchInputType::Boolean => "true/false",
                    DispatchInputType::Number => "number",
                    DispatchInputType::Choice => "one option",
                    DispatchInputType::Environment => "environment",
                },
                if input.required { " / required" } else { "" }
            ),
            if input.default.is_some() {
                "default"
            } else if input.required {
                "set"
            } else {
                "omit"
            },
            false,
        )
        .map_err(|_| safe())?;
        mode.choices = if input.default.is_some() {
            vec!["default".into(), "set".into(), "omit".into()]
        } else {
            vec!["set".into(), "omit".into()]
        };
        let mut value = Field::text(
            &format!("input:{}", input.name),
            &format!(
                "{} • transient input value • masked; never saved",
                input.name
            ),
            "",
            false,
        )
        .map_err(|_| safe())?;
        value.secret = true;
        value.choices = match input.input_type {
            DispatchInputType::Boolean => vec!["false".into(), "true".into()],
            DispatchInputType::Choice => input.options.clone(),
            _ => vec![],
        };
        fields.extend([mode, value]);
    }
    Ok(fields)
}
pub(crate) fn open(
    intent: Arc<ScopedDispatchIntentV7>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<Arc<OpenedDispatchV7>> {
    shared::clock(cancel, deadline)?;
    let local = LocalV3::capture(database, intent.source())?;
    let phase = provider.clone().with_catalogue_read_deadline(deadline);
    let mut session = phase.begin_repository_catalogue_access(
        intent.source.original().source(),
        &shared::access_limits(),
        cancel,
    )?;
    shared::scope(intent.source(), session.identity())?;
    session.verify(intent.source.reference(), cancel)?;
    let window = session.reserve_dispatch_reads_v7(intent.original(), 8, 1, cancel)?;
    let measured = phase.clone().with_measured_read_window(window)?;
    let current = session
        .observed_original_v1(intent.source.reference().id)
        .ok_or_else(safe)?;
    if let Some(pr) = intent.source.pr_origin_v2() {
        pr.fresh_parent(&measured, session.identity(), current, cancel)?;
    }
    let target =
        measured.scoped_actions_workflow_snapshot_v6(current, intent.original(), true, cancel)?;
    if target.model().state != "active" {
        return Err(safe());
    }
    let fields = base_fields(
        None,
        DispatchProtocolV7::Compatible2022Acceptance204,
        target.repository().default_branch(),
    )?;
    let mut workspace = ActionsWorkspace::new(
        session.identity().clone(),
        vec![current.repository().full_name.clone()],
    );
    workspace.workflows = vec![target.model().clone()];
    let seed = Form::actions_fields_v3(
        workspace,
        ActionsCommand::DispatchWorkflow,
        intent.key().into(),
        fields,
    );
    let lines=vec!["ORIGINAL WORKFLOW DISPATCH • inspect exact schema before Preview".into(),format!("Numeric repository {} • workflow {} • path {}",target.repository().id(),target.model().id,target.model().path),"Choose exact branch/tag namespace, literal name and API version. No defaults or host capabilities are guessed.".into(),"Inspect schema sends no POST. Default / Omit means not sent; actual GitHub defaults may apply.".into(),"Inputs and YAML are redacted from diagnostics; input bytes/digests are never saved.".into()];
    let display =
        crate::scoped_actions_workflow_publication_v6::display_v6(&lines, cancel, deadline)?;
    let retained = std::mem::size_of::<OpenedDispatchV7>()
        // Form stores this purpose in a Box. Count the complete display state,
        // including its tiny non-authoritative outcome annotation, once here.
        + std::mem::size_of::<crate::scoped_actions_dispatch_ui_v7::DispatchComposerStateV7>()
        + local.owned_bytes_v6()
        + identity_bytes(session.identity())
        + line_bytes(&lines)
        + display.retained_bytes()
        + field_bytes(&seed.fields)
        + std::mem::size_of::<Form>()
        + target.owned_bytes_v2()
        + target.repository().owned_bytes_v2();
    capacity(retained, display.preparation_peak_bound())?;
    measured.scoped_actions_workflow_snapshot_v6(current, &target, true, cancel)?;
    if let Some(pr) = intent.source.pr_origin_v2() {
        pr.fresh_parent(&measured, session.identity(), current, cancel)?;
    }
    session.verify(intent.source.reference(), cancel)?;
    let closed = session.close_with_guard(cancel, || {
        local.validate(database, intent.source())?;
        shared::clock(cancel, deadline)
    })?;
    shared::scope(intent.source(), closed.identity())?;
    let result = Arc::new(OpenedDispatchV7 {
        intent,
        local,
        identity: closed.identity().clone(),
        lines,
        display,
        seed: Mutex::new(Some(seed)),
        retained,
    });
    shared::clock(cancel, deadline)?;
    Ok(result)
}
pub(crate) fn prepare_schema(
    opened: Arc<OpenedDispatchV7>,
    choice: DispatchRefChoiceV7,
    protocol: DispatchProtocolV7,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<PreparedDispatchSchemaV7> {
    prepare(
        opened, choice, protocol, None, provider, database, cancel, deadline,
    )
    .and_then(|prepared| match prepared {
        PreparedPhaseV7::Schema(value) => Ok(*value),
        _ => Err(safe()),
    })
}
pub(crate) fn prepare_preview(
    request: &crate::scoped_actions_dispatch_ui_v7::DispatchRequestV7,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<Arc<DispatchPreviewV7>> {
    use crate::scoped_actions_dispatch_ui_v7::DispatchPhaseV7;
    let original = request.schema().ok_or_else(safe)?;
    if request.phase() != DispatchPhaseV7::Preview
        || request
            .summary_v152()
            .is_none_or(|summary| !Arc::ptr_eq(summary.schema(), original))
        || request.inputs().is_none()
    {
        return Err(safe());
    }
    prepare(
        request.opened().clone(),
        original.choice().clone(),
        original.protocol(),
        Some(request),
        provider,
        database,
        cancel,
        deadline,
    )
    .and_then(|prepared| match prepared {
        PreparedPhaseV7::Preview(value) => Ok(value),
        _ => Err(safe()),
    })
}
/// Mechanical allocation accounting only; this does not validate a source,
/// renew a phase or grant permission. Include a still-retained prior Preview
/// even though normal editable Back/input has already retired it.
pub(crate) fn original_snapshot_bytes_v153(
    request: &crate::scoped_actions_dispatch_ui_v7::DispatchRequestV7,
) -> GithubResult<usize> {
    let summary = request.summary_v152().ok_or_else(safe)?;
    let previous = request
        .preview()
        .map_or(Ok(0), |preview| preview.new_material_bytes_v152())?;
    request
        .opened()
        .retained()
        .checked_add(summary.retained_bytes())
        .and_then(|n| n.checked_add(request.form_bytes_v152()))
        .and_then(|n| {
            n.checked_add(std::mem::size_of_val(request) + 2 * std::mem::size_of::<usize>())
        })
        .and_then(|n| n.checked_add(previous))
        .ok_or_else(safe)
}
enum PreparedPhaseV7 {
    Schema(Box<PreparedDispatchSchemaV7>),
    Preview(Arc<DispatchPreviewV7>),
}
#[allow(clippy::too_many_arguments)]
fn prepare(
    opened: Arc<OpenedDispatchV7>,
    choice: DispatchRefChoiceV7,
    protocol: DispatchProtocolV7,
    request: Option<&crate::scoped_actions_dispatch_ui_v7::DispatchRequestV7>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<PreparedPhaseV7> {
    shared::clock(cancel, deadline)?;
    opened.validate_local(database)?;
    let source = opened.intent.source();
    let phase = provider.clone().with_catalogue_read_deadline(deadline);
    let mut session = phase.begin_repository_catalogue_access(
        source.original().source(),
        &shared::access_limits(),
        cancel,
    )?;
    shared::scope(source, session.identity())?;
    session.verify(source.reference(), cancel)?;
    let window = session.reserve_dispatch_reads_v7(
        opened.intent.original(),
        DISPATCH_POOL_CHILDREN_V7,
        1,
        cancel,
    )?;
    let measured = phase.clone().with_measured_read_window(window)?;
    let current = session
        .observed_original_v1(source.reference().id)
        .ok_or_else(safe)?;
    if let Some(pr) = source.pr_origin_v2() {
        pr.fresh_parent(&measured, session.identity(), current, cancel)?;
    }
    let versions = measured.scoped_dispatch_versions_v7(protocol, cancel)?;
    let cli = GhCliVersion::parse_line(&session.identity().gh_version).map_err(|_| safe())?;
    let schema = measured.scoped_dispatch_schema_v7(
        current,
        opened.intent.original(),
        choice,
        &versions,
        &cli,
        cancel,
    )?;
    if request.is_some_and(|request| request.schema().is_none_or(|prior| !prior.same(&schema))) {
        return Err(safe());
    }
    let result = if let Some(request) = request {
        let inputs = request.inputs().ok_or_else(safe)?.clone();
        let summary = request.summary_v152().ok_or_else(safe)?.clone();
        let original_snapshot = original_snapshot_bytes_v153(request)?;
        let before_plan = original_snapshot
            .checked_add(schema.retained_bytes())
            .and_then(|n| n.checked_add(inputs.retained_bytes()))
            .ok_or_else(safe)?;
        // The SAME planner clones definition twice. Its existing preflight
        // bounds that expansion; include both copies before invoking it.
        let definition =
            crate::actions_workspace::scoped_resources_v2::definition_bytes(schema.definition());
        checked_capture_capacity_v152(
            before_plan,
            definition
                .checked_mul(2)
                .and_then(|n| n.checked_add(256 * 1024))
                .ok_or_else(safe)?,
        )?;
        let names = dispatch_environment_names_v7(&schema, &inputs)?;
        let environments = names
            .iter()
            .map(|name| measured.scoped_dispatch_environment_v7(&schema, name, cancel))
            .collect::<GithubResult<Vec<_>>>()?;
        let (plan, payload) = ActionsPlan::prepare_dispatch_native_v7(
            session.identity(),
            &schema,
            &inputs,
            &environments,
        )?;
        plan.scoped_dispatch_descriptor_v7(&measured, source, &schema)?;
        let before_display = before_plan
            .checked_add(plan.scoped_dispatch_owned_bytes_v7()?)
            .and_then(|n| n.checked_add(payload.len()))
            .and_then(|n| {
                n.checked_add(
                    std::mem::size_of::<DispatchPreviewV7>() + 12 * std::mem::size_of::<usize>(),
                )
            })
            .ok_or_else(safe)?;
        display_preflight_v152(&plan.preview, before_display)?;
        let display = crate::scoped_actions_workflow_publication_v6::display_v6(
            &plan.preview,
            cancel,
            deadline,
        )?;
        let retained = before_display
            .checked_add(display.retained_bytes())
            .ok_or_else(safe)?;
        capacity(retained, display.preparation_peak_bound())?;
        PreparedPhaseV7::Preview(Arc::new(DispatchPreviewV7 {
            opened: opened.clone(),
            schema: schema.clone(),
            inputs,
            plan,
            payload,
            display,
            attempted: AtomicBool::new(false),
            summary,
        }))
    } else {
        // Editable fields clone only bounded input names/options from this
        // actual immutable definition; preflight before those copies too.
        checked_capture_capacity_v152(
            opened.retained() + schema.retained_bytes(),
            crate::actions_workspace::scoped_resources_v2::definition_bytes(schema.definition())
                .checked_add(128 * 1024)
                .ok_or_else(safe)?,
        )?;
        let fields = fields(&schema)?;
        let lines=vec!["EXACT DISPATCH SCHEMA • summary only; full details in Preview".into(),format!("{} {} • chosen tip {} ({})",schema.choice().label(),schema.choice().name(),schema.tip(),schema.ref_type()),format!("Actual definition commit: {}",schema.commit()),format!("Protocol: {}",schema.protocol().label()),format!("Declared inputs: {} • choose explicit modes and transient values",schema.definition().dispatch_inputs.len()),"Default / Omit: not sent; GitHub may apply actual YAML defaults. Values/defaults are not repeated here or saved.".into()];
        let before_display = opened.retained()
            + schema.retained_bytes()
            + field_bytes(&fields)
            + line_bytes(&lines)
            + std::mem::size_of::<DispatchEditableSummaryV152>()
            + std::mem::size_of::<PreparedDispatchSchemaV7>()
            + 8 * std::mem::size_of::<usize>();
        display_preflight_v152(&lines, before_display)?;
        let display =
            crate::scoped_actions_workflow_publication_v6::display_v6(&lines, cancel, deadline)?;
        let retained = before_display + display.retained_bytes();
        capacity(retained, display.preparation_peak_bound())?;
        PreparedPhaseV7::Schema(Box::new(PreparedDispatchSchemaV7 {
            opened: opened.clone(),
            fields,
            summary: Arc::new(DispatchEditableSummaryV152 {
                schema: schema.clone(),
                lines: lines.into(),
                display,
            }),
        }))
    };
    measured.validate_dispatch_ref_v7(&schema, cancel)?;
    measured.validate_dispatch_workflow_v7(&schema, cancel)?;
    if let Some(pr) = source.pr_origin_v2() {
        pr.fresh_parent(&measured, session.identity(), current, cancel)?;
    }
    session.verify(source.reference(), cancel)?;
    let closed = session.close_with_guard(cancel, || {
        opened.validate_local(database)?;
        shared::clock(cancel, deadline)
    })?;
    shared::scope(source, closed.identity())?;
    shared::clock(cancel, deadline)?;
    Ok(result)
}
