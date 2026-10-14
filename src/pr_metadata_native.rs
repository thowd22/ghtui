//! Native structured PR metadata. Opaque read snapshots are ephemeral choices,
//! not credentials or write authority. Every durable original is resolved before
//! edited values; workers verify the positive current principal around all reads.
use crate::{
    drafts::DraftClient,
    forms::{Form, FormPurpose},
    github_workspace::GithubWorkspace,
    pr_actions::PrActionContext,
    pr_metadata_actions::{
        self, PrMetadataCommand, PrMetadataExecution, PrMetadataExecutionContext, PrMetadataPlan,
        PrMetadataRefresh,
    },
    pr_metadata_catalogue::{self, LoadedPrMetadataChoices},
    pr_metadata_choices::{PrMetadataLimits, PrMetadataOriginal},
    pr_metadata_picker::PrMetadataPicker,
    pr_write_journal::{PrWriteJournal, PrWriteMarker, PrWriteState, PrWriteTarget},
    providers::{
        gh::{GithubErrorKind, GithubProvider, HostIdentity, PrMetadataReadLimits, PrSummary},
        runner::CancellationToken,
    },
    runtime::PreparedView,
    storage::{Cache, Scope},
};
use std::{collections::BTreeMap, path::Path, sync::Arc};

#[derive(Clone)]
pub struct PrMetadataIntent {
    pub command: PrMetadataCommand,
    pub identity: HostIdentity,
    pub original: Arc<PrSummary>,
    source: Option<Arc<LoadedPrMetadataChoices>>,
    selected_index: Option<usize>,
    selected_value: String,
    pub plan: Option<Arc<PrMetadataPlan>>,
    pub preview_lines: Vec<String>,
}
impl std::fmt::Debug for PrMetadataIntent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrMetadataIntent")
            .field("command", &self.command)
            .field("source", &self.source.is_some())
            .field("selection", &self.selected_index)
            .field("plan", &self.plan.is_some())
            .finish()
    }
}
impl PrMetadataIntent {
    pub fn new(
        command: PrMetadataCommand,
        identity: HostIdentity,
        original: PrSummary,
    ) -> Result<Self, String> {
        PrWriteTarget::new(&identity, &original).map_err(|e| e.to_string())?;
        Ok(Self {
            command,
            identity,
            original: Arc::new(original),
            source: None,
            selected_index: None,
            selected_value: String::new(),
            plan: None,
            preview_lines: vec![],
        })
    }
    pub fn clear_selection(&mut self) {
        self.source = None;
        self.selected_index = None;
        self.selected_value.clear();
        self.plan = None;
        self.preview_lines.clear();
    }
    /// Local choose carries an original index from this same retained widget.
    /// It performs no source parsing, transport or publication preparation.
    pub fn choose(&mut self, picker: &PrMetadataPicker, index: usize) -> Result<String, String> {
        if !same(&self.identity, picker.source().identity())
            || !same_original(&self.original, picker.source().target())
        {
            return Err(
                "Chosen metadata source differs from the original account/PR/head/base".into(),
            );
        }
        let label = if self.command.adds() {
            if picker.current_command().is_some() {
                return Err("Eligible addition cannot use a current-member index".into());
            }
            picker.source().choose(index).map_err(|e| e.to_string())?;
            picker
                .source()
                .prepared()
                .choices()
                .get(index)
                .ok_or("No observed choice at this index")?
                .label()
                .to_owned()
        } else {
            if picker.current_command() != Some(self.command) {
                return Err("Removal requires this command's genuine current-member index".into());
            }
            let member = picker
                .current_member(index)
                .ok_or("Current observed member is unavailable")?;
            format!("{} (#{})", member.name(), member.id())
        };
        if label.len() > 8192 {
            return Err("Chosen display label exceeds its byte budget".into());
        }
        self.source = Some(picker.source().clone());
        self.selected_index = Some(index);
        self.selected_value = label.clone();
        self.plan = None;
        self.preview_lines.clear();
        Ok(label)
    }
    pub fn prepare(&mut self, values: &BTreeMap<String, String>) -> Result<(), String> {
        self.validate_values(values)?;
        let source = self
            .source
            .as_ref()
            .ok_or("Choose F4 and a genuine observed value before Preview")?;
        let plan = Arc::new(PrMetadataPlan::prepare(
            source.source().clone(),
            self.command,
            self.selected_index,
        )?);
        let lines = preview(&plan)?;
        self.plan = Some(plan);
        self.preview_lines = lines;
        Ok(())
    }
    fn validate_values(&self, values: &BTreeMap<String, String>) -> Result<(), String> {
        let source = self
            .source
            .as_ref()
            .ok_or("Choose F4 and a genuine observed value before Preview")?;
        if values.len() != 1
            || values.get("body") != Some(&self.selected_value)
            || !same(&self.identity, source.identity())
            || !same_original(&self.original, source.target())
        {
            return Err(
                "Original account/PR/head/base or chosen field changed; choose and Preview again"
                    .into(),
            );
        }
        Ok(())
    }
    pub fn validate_plan(
        &self,
        values: &BTreeMap<String, String>,
    ) -> Result<Arc<PrMetadataPlan>, String> {
        self.validate_values(values)?;
        let plan = self
            .plan
            .as_ref()
            .ok_or("Choose Preview and inspect the exact effect before confirmation")?;
        if plan.command() != self.command
            || !Arc::ptr_eq(plan.source_arc(), self.source.as_ref().unwrap().source())
        {
            return Err("Metadata preview belongs to another source/command; preview again".into());
        }
        Ok(plan.clone())
    }
}
fn same(expected: &HostIdentity, actual: &HostIdentity) -> bool {
    Scope::github(expected.host.to_ascii_lowercase(), expected.account_id)
        .ok()
        .zip(Scope::github(actual.host.to_ascii_lowercase(), actual.account_id).ok())
        .is_some_and(|(a, b)| a == b)
}
fn same_original(a: &PrSummary, b: &PrSummary) -> bool {
    a.pr.host.eq_ignore_ascii_case(&b.pr.host)
        && a.pr.repository.eq_ignore_ascii_case(&b.pr.repository)
        && a.pr.id == b.pr.id
        && a.pr.number == b.pr.number
        && a.head_sha == b.head_sha
        && a.base_sha == b.base_sha
        && a.head_ref == b.head_ref
        && a.base_ref == b.base_ref
}
fn hidden(error: impl std::fmt::Display) -> PreparedView {
    PreparedView {
        clear_private: true,
        status: format!("PR metadata identity/access unavailable • private pages hidden: {error}"),
        ..Default::default()
    }
}
fn verify(
    provider: &GithubProvider,
    identity: &HostIdentity,
    cancel: &CancellationToken,
) -> Result<(), Box<PreparedView>> {
    match provider.identity(cancel) {
        Ok(actual) if same(identity, &actual) => Ok(()),
        Ok(_) => Err(Box::new(hidden("positive numeric account changed"))),
        Err(error) => Err(Box::new(hidden(error))),
    }
}
fn preview(plan: &PrMetadataPlan) -> Result<Vec<String>, String> {
    let raw = plan
        .before()
        .iter()
        .chain(plan.after())
        .chain(plan.preserved_review_members());
    let mut bytes = 0usize;
    for member in raw {
        bytes = bytes
            .checked_add(
                member.name().len() + member.node().len() + member.namespace().map_or(0, str::len),
            )
            .ok_or("Metadata preview size overflow")?;
        if member.name().len() > 4096 || member.node().len() > 512 || bytes > 128 * 1024 {
            return Err("Metadata preview exceeds its raw byte bound".into());
        }
    }
    let mut rows = vec![
        format!("Exact effect: {}", plan.command().label()),
        format!(
            "Target: {}#{}",
            plan.target().repository(),
            plan.target().number()
        ),
        format!(
            "Actor: @{} • numeric account #{} • {}",
            plan.identity().login,
            plan.identity().account_id.unwrap_or_default(),
            plan.identity().host
        ),
        "Original observed head:".into(),
        plan.original().head_sha.clone(),
        format!("Original base: {}", plan.original().base_ref),
        "Current selected field:".into(),
    ];
    let show = |member: &pr_metadata_actions::PrMetadataMember| {
        format!(
            "{} • database #{} • node {}",
            member.name(),
            member.id(),
            member.node()
        )
    };
    if plan.before().is_empty() {
        rows.push("Known empty selected field".into());
    } else {
        rows.extend(plan.before().iter().map(show));
    }
    rows.push("Requested selected field:".into());
    if plan.after().is_empty() {
        rows.push("Explicit known empty selected field".into());
    } else {
        rows.extend(plan.after().iter().map(show));
    }
    if plan.command().aggregate_review_replacement() {
        rows.push("Complete User+Team replacement • union=false".into());
        rows.push("Preserved opposite review-request set:".into());
        if plan.preserved_review_members().is_empty() {
            rows.push("Explicit known empty opposite set".into());
        } else {
            rows.extend(plan.preserved_review_members().iter().map(show));
        }
    }
    rows.extend([plan.command().operation_notice().into(),"Observed snapshot only; current permission, genuine IDs and complete sets are verified before any journal claim/write.".into(),"Local files, checkout, index and commits remain unchanged.".into(),"Confirm action publishes this exact metadata intent once. Drafts never retain approval.".into()]);
    for member in plan
        .before()
        .iter()
        .chain(plan.after())
        .chain(plan.preserved_review_members())
    {
        if crate::app::safe_text(member.name()) != member.name() {
            let escaped = member
                .name()
                .chars()
                .flat_map(char::escape_debug)
                .collect::<String>();
            if escaped.len() > 16 * 1024 {
                return Err("Exact escaped identifier exceeds preview byte budget".into());
            }
            rows.push(format!("Exact raw name (escaped): {escaped}"));
        }
    }
    let mut out = vec![];
    let mut prepared = 0usize;
    for row in rows {
        for line in crate::ui::notice_lines(&crate::app::safe_text(&row), 60) {
            let text = line
                .spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>();
            prepared = prepared.saturating_add(text.len());
            if out.len() >= 4096 || prepared > 256 * 1024 {
                return Err("Prepared metadata preview budget exceeded".into());
            }
            out.push(text);
        }
    }
    Ok(out)
}

/// OPEN brackets all private draft/original SQL with a fresh positive principal.
pub fn open(
    command: PrMetadataCommand,
    context: PrActionContext,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    if let Err(view) = verify(provider, &context.identity, cancel) {
        return Ok(*view);
    }
    let read = (|| {
        let intent =
            PrMetadataIntent::new(command, context.identity.clone(), context.target.clone())?;
        let mut form = Form::pr_metadata(intent)?;
        let cache = Cache::open(database).map_err(|e| e.to_string())?;
        if let Some(body) = cache
            .bounded_draft(&form.scope(), &form.resource(), 32 * 1024)
            .map_err(|e| e.to_string())?
        {
            form.restore_draft(&body)?;
        }
        if form.publication.is_some() {
            prepare_original_sql(&mut form, database)?;
        } else {
            let target = PrWriteTarget::new(&context.identity, &context.target)
                .map_err(|e| e.to_string())?;
            if PrWriteJournal::open(database)
                .map_err(|e| e.to_string())?
                .blocking(&target)
                .map_err(|e| e.to_string())?
                .is_some()
            {
                form.error="Unresolved original PR publication blocks a new write; inspect it explicitly. Nothing resent.".into();
            }
        }
        Ok::<_, String>(form)
    })();
    if let Err(view) = verify(provider, &context.identity, cancel) {
        return Ok(*view);
    }
    let form = read?;
    Ok(PreparedView {context:Some(context.workspace.context()),github:Some(context.workspace),form:Some(form),status:"Local PR metadata intent ready • choose observed value, then Preview; nothing published".into(),clear_pr_inspection:true,..Default::default()})
}
fn prepare_original_sql(form: &mut Form, database: &Path) -> Result<(), String> {
    let FormPurpose::PrMetadata(intent) = &form.purpose else {
        return Err("Not a structured metadata original".into());
    };
    let target =
        PrWriteTarget::new(&intent.identity, &intent.original).map_err(|e| e.to_string())?;
    // Repair unknown/confirmed label visibility BEFORE showing original metadata.
    let fence = pr_metadata_actions::repair_label_filter_reads(
        database,
        &intent.identity,
        &intent.original,
    )?;
    let marker = PrWriteJournal::open(database)
        .map_err(|e| e.to_string())?
        .original_publication(
            &target,
            form.publication
                .as_ref()
                .ok_or("Original reference missing")?,
        )
        .map_err(|e| e.to_string())?
        .ok_or("Original publication is unavailable; do not resend")?;
    if marker.command != intent.command.key() {
        return Err("Recorded original command differs from this form; do not resend".into());
    }
    if marker.state == PrWriteState::Confirmed {
        pr_metadata_actions::validate_recorded_marker(&marker)?;
    }
    fence.validate()?;
    form.metadata_original_lines = original_lines(&marker);
    form.invalidate_plan();
    form.preview = true;
    form.preview_scroll = 0;
    form.submitting = false;
    form.error.clear();
    Ok(())
}
fn original_lines(marker: &PrWriteMarker) -> Vec<String> {
    let state = match marker.state {
        PrWriteState::Confirmed => "Confirmed • do not resend",
        PrWriteState::Pending => "Pending • invocation/effect unverified; do not resend",
        PrWriteState::Uncertain => "Uncertain • do not resend",
        PrWriteState::Acknowledged => "Acknowledged • no effect confirmation implied",
    };
    let mut lines = vec![
        "Original PR metadata publication record • read-only".into(),
        format!("Recorded state: {state}"),
        format!("Recorded command: {}", marker.command),
        format!("Original attempt: {}", marker.attempt_id),
        format!(
            "Target: {}#{}",
            marker.target.repository(),
            marker.target.number()
        ),
        "Original observed head:".into(),
        marker.observed_head.clone(),
        "No catalogue lookup, retry, publication or acknowledgement occurs here.".into(),
        "F9 New intent clears only this local reference; durable protection remains.".into(),
    ];
    if let Some(effect) = &marker.confirmed {
        if let Ok(receipt) = pr_metadata_actions::PrMetadataReceipt::decode(effect) {
            lines.push(format!(
                "Confirmed actual selected database ID: {}",
                receipt
                    .selected_id()
                    .map_or_else(|| "known empty".into(), |id| id.to_string())
            ));
            lines.push(format!("Confirmed result IDs: {:?}", receipt.result_ids()));
        }
    }
    lines
}
pub fn original(
    mut form: Form,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    let FormPurpose::PrMetadata(intent) = &form.purpose else {
        return Err("Not a structured metadata original".into());
    };
    let identity = intent.identity.clone();
    if let Err(view) = verify(provider, &identity, cancel) {
        return Ok(*view);
    }
    let result = prepare_original_sql(&mut form, database);
    if let Err(view) = verify(provider, &identity, cancel) {
        return Ok(*view);
    }
    if let Err(error) = result {
        form.invalidate_plan();
        form.metadata_original_lines =
            vec!["Original PR metadata record unavailable • do not resend".into()];
        form.preview = true;
        form.submitting = false;
        form.error = error;
    }
    Ok(PreparedView {
        form: Some(form),
        status: "Original PR metadata publication record • nothing resent or acknowledged".into(),
        ..Default::default()
    })
}
pub fn load_choices(
    mut form: Form,
    workspace: Option<&GithubWorkspace>,
    provider: &GithubProvider,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    let FormPurpose::PrMetadata(intent) = &form.purpose else {
        return Err("Not a structured metadata chooser".into());
    };
    let identity = intent.identity.clone();
    let original = intent.original.clone();
    let command = intent.command;
    if form.publication.is_some() {
        return Err("Original viewer never reloads eligible metadata".into());
    }
    let context = if let Some(context) = workspace
        .and_then(|w| w.pr.as_ref())
        .and_then(|p| p.context.clone())
    {
        Ok(context)
    } else {
        if let Err(view) = verify(provider, &identity, cancel) {
            return Ok(*view);
        }
        provider.pr_context(&original, cancel)
    };
    let result = context.and_then(|context| {
        pr_metadata_catalogue::load(
            provider,
            PrMetadataOriginal {
                identity: &identity,
                target: &original,
                context: &context,
            },
            command.catalogue(),
            &PrMetadataReadLimits::default(),
            PrMetadataLimits::default(),
            cancel,
        )
    });
    if let Err(view) = verify(provider, &identity, cancel) {
        return Ok(*view);
    }
    form.invalidate_plan();
    form.submitting = false;
    match result {
        Err(error)
            if error.repository_access_lost()
                || matches!(
                    error.kind,
                    GithubErrorKind::IdentityChanged | GithubErrorKind::Authentication
                ) =>
        {
            return Ok(hidden(error))
        }
        Err(error) => {
            form.error = error.to_string();
        }
        Ok(source) => {
            let source = Arc::new(source);
            let FormPurpose::PrMetadata(intent) = &mut form.purpose else {
                unreachable!()
            };
            intent.clear_selection();
            if command == PrMetadataCommand::ClearMilestone {
                intent.source = Some(source);
                intent.selected_value = "Clear current milestone".into();
                form.fields[0].editor = crate::editor::TextBuffer::new(&intent.selected_value)?;
                form.dirty = true;
            } else if command.adds() {
                form.metadata_picker = Some(PrMetadataPicker::new(source));
            } else {
                form.metadata_picker = Some(PrMetadataPicker::current(source, command)?);
            }
            form.error.clear();
        }
    }
    Ok(PreparedView {
        status: if form.error.is_empty() {
            "PR metadata choices ready • browsing changes nothing".into()
        } else {
            "PR metadata choices unavailable • no source, marker or write created".into()
        },
        form: Some(form),
        ..Default::default()
    })
}
pub fn preview_form(mut form: Form) -> Result<PreparedView, String> {
    let values = form.values();
    let FormPurpose::PrMetadata(intent) = &mut form.purpose else {
        return Err("Not a structured metadata preview".into());
    };
    intent.prepare(&values)?;
    form.preview = true;
    form.preview_scroll = 0;
    form.submitting = false;
    form.error.clear();
    Ok(PreparedView {
        form: Some(form),
        status: "Read-only metadata preview ready • nothing changed until confirmation".into(),
        ..Default::default()
    })
}
pub fn submit(
    mut form: Form,
    workspace: GithubWorkspace,
    provider: &GithubProvider,
    database: &Path,
    drafts: &DraftClient,
    pending: Vec<(Scope, String, String)>,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    if form.publication.is_some() {
        return original(form, provider, database, cancel);
    }
    let FormPurpose::PrMetadata(intent) = &form.purpose else {
        return Err("Not a structured metadata publication".into());
    };
    let identity = intent.identity.clone();
    let command = intent.command;
    if let Err(view) = verify(provider, &identity, cancel) {
        return Ok(*view);
    }
    drafts.save_pending(
        pending
            .into_iter()
            .filter(|(scope, _, _)| *scope == form.scope())
            .collect(),
    )?;
    let prepared = intent.validate_plan(&form.values());
    let mut refreshed = None;
    let result = match prepared {
        Err(error) => PrMetadataExecution::Refused { error },
        Ok(plan) => {
            let mut journal = PrWriteJournal::open(database).map_err(|e| e.to_string())?;
            plan.execute(
                PrMetadataExecutionContext {
                    provider,
                    journal: &mut journal,
                    confirmed: true,
                    restored: None,
                    limits: &PrMetadataReadLimits::default(),
                    cancel,
                },
                |marker| {
                    form.publication = Some(marker.publication_reference());
                    drafts.save_and_wait(form.scope(), form.resource(), form.draft())
                },
                |_, _| {
                    let observation = provider
                        .inspect_pr_target(&identity, plan.original(), cancel)
                        .map_err(|e| e.to_string())?;
                    let fresh = workspace
                        .clone()
                        .replace_pr_details_with_current_context(
                            observation.details().clone(),
                            provider,
                            cancel,
                        )
                        .map_err(|e| e.to_string())?;
                    refreshed = Some(fresh);
                    Ok(PrMetadataRefresh {
                        private_visible: true,
                    })
                },
            )
        }
    };
    if let Err(mut view) = verify(provider, &identity, cancel) {
        let effect = match &result {
            PrMetadataExecution::Confirmed { .. }
            | PrMetadataExecution::ConfirmedButJournalFailed { .. }
            | PrMetadataExecution::ConfirmedButRefreshFailed { .. }
            | PrMetadataExecution::RestoredAlreadyConfirmed { .. } => {
                "Published • private pages hidden • do not resend"
            }
            PrMetadataExecution::Uncertain { .. } => {
                "Publication outcome unknown • private pages hidden • do not resend"
            }
            PrMetadataExecution::Refused { .. } | PrMetadataExecution::RefusedPrivate { .. } => {
                "PR metadata refused • private pages hidden • nothing resent"
            }
        };
        view.status = format!("{effect} • current numeric identity unavailable");
        return Ok(*view);
    }
    let mut visible = false;
    let mut changed = false;
    let status = match result {
        PrMetadataExecution::RefusedPrivate { error } => {
            return Ok(PreparedView {
                clear_private: true,
                status: format!(
                    "PR metadata source unavailable • private pages hidden • nothing sent: {error}"
                ),
                ..Default::default()
            })
        }
        PrMetadataExecution::Refused { error } => {
            form.error = error;
            form.submitting = false;
            "PR metadata intent refused • nothing resent".into()
        }
        PrMetadataExecution::RestoredAlreadyConfirmed { marker } => {
            form.publication = Some(marker.publication_reference());
            form.metadata_original_lines = original_lines(&marker);
            form.preview = true;
            form.submitting = false;
            "Original metadata confirmed • do not resend".into()
        }
        PrMetadataExecution::Confirmed {
            publication,
            private_visible,
            ..
        } => {
            form.publication = Some(publication.publication_reference());
            form.metadata_original_lines = original_lines(&publication);
            // Keep the structured original visible after completion. Its
            // action now opens read-only history, never the just-used plan.
            form.invalidate_plan();
            form.preview = true;
            form.submitted = false;
            form.submitting = false;
            form.dirty = false;
            visible = private_visible;
            changed = true;
            match drafts.remove(form.scope(), form.resource()) {
                Ok(()) => format!("Published • {} • confirmed once", command.label()),
                Err(error) => {
                    form.error =
                        format!("Draft cleanup failed; original reference retained • {error}");
                    format!(
                        "Published • {} • confirmed; local draft retained, do not resend",
                        command.label()
                    )
                }
            }
        }
        PrMetadataExecution::Uncertain { publication, error } => {
            changed = true;
            retain_actual_original(&mut form, &publication, database);
            form.error = error;
            form.submitting = false;
            "Publication outcome unknown • private pages hidden • do not resend".into()
        }
        PrMetadataExecution::ConfirmedButJournalFailed {
            publication, error, ..
        }
        | PrMetadataExecution::ConfirmedButRefreshFailed {
            publication, error, ..
        } => {
            changed = true;
            retain_actual_original(&mut form, &publication, database);
            form.error = error;
            form.submitting = false;
            "Published • private pages hidden • do not resend".into()
        }
    };
    Ok(PreparedView {
        context: visible.then(|| refreshed.as_ref().unwrap().context()),
        github: if visible { refreshed } else { None },
        form: Some(form),
        status,
        clear_pr_inspection: true,
        invalidate_metrics: changed,
        invalidate_pr_metadata: changed,
        ..Default::default()
    })
}
fn retain_actual_original(form: &mut Form, marker: &PrWriteMarker, database: &Path) {
    form.publication = Some(marker.publication_reference());
    // Recapture actual durable state; an initial Pending clone is not a receipt.
    match PrWriteJournal::open(database).and_then(|journal| {
        journal.original_publication(&marker.target, &marker.publication_reference())
    }) {
        Ok(Some(actual)) => form.metadata_original_lines = original_lines(&actual),
        _ => {
            form.metadata_original_lines = vec![
                "Original durable state unavailable • do not resend; no receipt inferred".into(),
            ]
        }
    }
    form.preview = true;
    form.dirty = false;
}
