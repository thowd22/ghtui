//! Native Base selection is ephemeral, original-bound read evidence. It cannot
//! be deserialized or grant mutation authority; fresh exact reads precede every
//! new publication, followed by the existing durable original/effect guards.
use crate::{
    forms::{Form, FormPurpose, PrFormKind, PrIntent},
    github_workspace::GithubWorkspace,
    pr_metadata_catalogue::{self, LoadedPrMetadataChoices},
    pr_metadata_choices::{
        PrMetadataLimits, PrMetadataOriginal, PrMetadataSelection, PrMetadataValue,
    },
    pr_metadata_picker::PrMetadataPicker,
    pr_write_journal::PrWriteTarget,
    providers::{
        gh::{
            GithubErrorKind, GithubProvider, GithubResult, MetadataChoices, PrMetadataReadLimits,
        },
        runner::CancellationToken,
    },
    runtime::PreparedView,
    storage::Scope,
};
use std::path::Path;
use std::{collections::BTreeMap, sync::Arc};

/// Exact original history is never rebuilt from edited draft values/current
/// head/tip. Neither viewing nor New intent acknowledges or releases the marker.
pub fn prepare_original(
    mut form: Form,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    let FormPurpose::Pr(intent) = &form.purpose else {
        return Err("Original Base record requires its PR form".into());
    };
    if intent.kind != PrFormKind::Base {
        return Err("Original record requires a Base metadata form".into());
    }
    let reference = form
        .publication
        .as_ref()
        .ok_or("Original Base reference is missing")?;
    let verified = provider.identity(cancel);
    match verified {
        Ok(actual) if same_principal(&intent.identity, &actual) => {}
        Ok(_) => {
            return Ok(hidden(
                "positive numeric principal differs before original history",
            ))
        }
        Err(error) => return Ok(hidden(error)),
    }
    let read = (|| {
        let target = PrWriteTarget::new(&intent.identity, &intent.target)
            .map_err(|error| error.to_string())?;
        let journal = crate::pr_write_journal::PrWriteJournal::open(database)
            .map_err(|error| error.to_string())?;
        let marker = journal
            .original_publication(&target, reference)
            .map_err(|error| error.to_string())?
            .ok_or("Original publication reference is unavailable; do not resend")?;
        if marker.command != "legacy-base" || marker.selected_resource != "pr" {
            return Err(
                "Recorded original is not this Base metadata operation; do not resend".into(),
            );
        }
        if let Some(effect) = &marker.confirmed {
            if effect.kind != crate::pr_write_journal::PrEffectKind::Metadata
                || effect.resource_id != target.node_id()
                || effect.node_id.as_deref() != Some(target.node_id())
            {
                return Err(
                    "Original Base metadata receipt differs from the exact PR; do not resend"
                        .into(),
                );
            }
        }
        let state = match marker.state {
            crate::pr_write_journal::PrWriteState::Confirmed => "Confirmed • do not resend",
            crate::pr_write_journal::PrWriteState::Pending => {
                "Pending request • invocation/effect unverified; do not resend"
            }
            crate::pr_write_journal::PrWriteState::Uncertain => "Uncertain outcome • do not resend",
            crate::pr_write_journal::PrWriteState::Acknowledged => {
                "Acknowledged • no effect confirmation implied"
            }
        };
        Ok(vec!["Original Base publication record • read-only".into(),format!("Recorded state: {state}"),
            format!("Original attempt: {}",marker.attempt_id),format!("Target: {}#{}",target.repository(),target.number()),
            "Original observed head:".into(),marker.observed_head,
            "Original requested text is not retained in the ledger; edited draft text is not a receipt.".into(),
            "No branch lookup, publication, retry or acknowledgement occurs in this viewer.".into(),
            "F9 New intent clears only this local reference. Original durable protection remains.".into()])
    })();
    let final_identity = provider.identity(cancel);
    match final_identity {
        Ok(actual) if same_principal(&intent.identity, &actual) => {}
        Ok(_) => {
            return Ok(hidden(
                "positive numeric principal differs after original history",
            ))
        }
        Err(error) => return Ok(hidden(error)),
    }
    form.invalidate_plan();
    form.submitting = false;
    form.preview = true;
    form.preview_scroll = 0;
    match read {
        Ok(lines) => {
            form.metadata_original_lines = lines;
            form.error.clear();
        }
        Err(error) => {
            form.metadata_original_lines =
                vec!["Original Base record unavailable • do not resend".into()];
            form.error = error;
        }
    }
    Ok(PreparedView {
        form: Some(form),
        status: "Original Base publication record • nothing resent or acknowledged".into(),
        ..Default::default()
    })
}

#[derive(Clone)]
pub struct PrBaseSelection {
    source: Arc<LoadedPrMetadataChoices>,
    selected: PrMetadataSelection,
}
impl std::fmt::Debug for PrBaseSelection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrBaseSelection(<original-bound ephemeral branch>)")
    }
}
impl PrBaseSelection {
    pub fn new(
        source: Arc<LoadedPrMetadataChoices>,
        index: usize,
        intent: &PrIntent,
    ) -> Result<Self, String> {
        let selected = source.choose(index).map_err(|e| e.to_string())?;
        let value = Self { source, selected };
        value.validate(
            intent,
            &BTreeMap::from([("body".into(), value.name()?.into())]),
        )?;
        Ok(value)
    }
    pub fn source(&self) -> &Arc<LoadedPrMetadataChoices> {
        &self.source
    }
    pub fn value(&self) -> &PrMetadataValue {
        self.selected.value()
    }
    pub fn name(&self) -> Result<&str, String> {
        match self.value() {
            PrMetadataValue::BaseBranch { name, .. } => Ok(name),
            _ => Err("The selected value is not a genuine Base branch".into()),
        }
    }
    pub fn tip(&self) -> Result<&str, String> {
        match self.value() {
            PrMetadataValue::BaseBranch { commit_sha, .. } => Ok(commit_sha),
            _ => Err("The selected value is not a genuine Base branch".into()),
        }
    }
    pub fn validate(
        &self,
        intent: &PrIntent,
        values: &BTreeMap<String, String>,
    ) -> Result<(), String> {
        let key =
            PrWriteTarget::new(&intent.identity, &intent.target).map_err(|e| e.to_string())?;
        let observed = self.source.target();
        if intent.kind != PrFormKind::Base
            || key.scope() != self.selected.target().scope()
            || !key
                .repository()
                .eq_ignore_ascii_case(self.selected.target().repository())
            || key.node_id() != self.selected.target().node_id()
            || key.number() != self.selected.target().number()
            || intent.target.head_sha != observed.head_sha
            || intent.target.base_sha != observed.base_sha
            || intent.target.head_ref != observed.head_ref
            || intent.target.base_ref != observed.base_ref
            || values.len() != 1
            || values.get("body").map(String::as_str) != Some(self.name()?)
        {
            return Err("Original chosen Base/account/PR/head/base/ref or raw input changed; choose and preview again".into());
        }
        self.selected.validate_original(PrMetadataOriginal {
            identity: self.source.identity(),
            target: observed,
            context: self.source.context(),
        })?;
        self.selected.validate_catalogue(self.source.prepared())
    }
}
fn same_principal(
    expected: &crate::providers::gh::HostIdentity,
    actual: &crate::providers::gh::HostIdentity,
) -> bool {
    Scope::github(expected.host.to_ascii_lowercase(), expected.account_id)
        .ok()
        .zip(Scope::github(actual.host.to_ascii_lowercase(), actual.account_id).ok())
        .is_some_and(|(a, b)| a == b)
}
fn hidden(error: impl std::fmt::Display) -> PreparedView {
    PreparedView {
        clear_private: true,
        status: format!("PR choices identity/access unavailable • private pages hidden: {error}"),
        ..Default::default()
    }
}
/// Final current-principal verification runs on both successful and failed
/// source reads. A failed/exhausted producer does not supply closing authority.
fn close_read<T>(
    provider: &GithubProvider,
    intent: &PrIntent,
    read: GithubResult<T>,
    cancel: &CancellationToken,
) -> Result<Result<T, String>, Box<PreparedView>> {
    match provider.identity(cancel) {
        Ok(actual) if same_principal(&intent.identity, &actual) => {}
        Ok(_) => return Err(Box::new(hidden("positive numeric principal differs"))),
        Err(error) => return Err(Box::new(hidden(error))),
    }
    match read {
        Err(error)
            if error.repository_access_lost()
                || matches!(
                    error.kind,
                    GithubErrorKind::IdentityChanged | GithubErrorKind::Authentication
                ) =>
        {
            Err(Box::new(hidden(error)))
        }
        value => Ok(value.map_err(|error| error.to_string())),
    }
}
pub fn prepare_picker(
    mut form: Form,
    workspace: &GithubWorkspace,
    provider: &GithubProvider,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    let FormPurpose::Pr(intent) = &form.purpose else {
        return Err("Base choices require an original PR form".into());
    };
    let page = workspace
        .pr
        .as_ref()
        .ok_or("Original PR page is no longer active")?;
    let context = if let Some(context) = &page.context {
        Ok(context.clone())
    } else {
        let verified = provider.identity(cancel);
        match verified {
            Ok(actual) if same_principal(&intent.identity, &actual) => {
                provider.pr_context(&intent.target, cancel)
            }
            Ok(_) => return Ok(hidden("positive numeric principal differs before context")),
            Err(error) => return Ok(hidden(error)),
        }
    };
    let read = context.and_then(|context| {
        pr_metadata_catalogue::load(
            provider,
            PrMetadataOriginal {
                identity: &intent.identity,
                target: &intent.target,
                context: &context,
            },
            MetadataChoices::Branches,
            &PrMetadataReadLimits::default(),
            PrMetadataLimits::default(),
            cancel,
        )
    });
    let result = match close_read(provider, intent, read, cancel) {
        Ok(value) => value,
        Err(hidden) => return Ok(*hidden),
    };
    form.submitting = false;
    form.invalidate_plan();
    match result {
        Ok(loaded) => {
            form.metadata_picker = Some(PrMetadataPicker::new(Arc::new(loaded)));
            form.error.clear();
            Ok(PreparedView {
                form: Some(form),
                status: "PR metadata choices ready • browsing changes nothing".into(),
                ..Default::default()
            })
        }
        Err(error) => {
            form.error = error.clone();
            Ok(PreparedView {
                form: Some(form),
                status: format!("PR choices unavailable • {error}"),
                ..Default::default()
            })
        }
    }
}
/// Return a prepared refusal without invoking or creating any durable attempt.
pub fn revalidate(
    form: &Form,
    provider: &GithubProvider,
    cancel: &CancellationToken,
) -> Result<(), Box<PreparedView>> {
    let FormPurpose::Pr(intent) = &form.purpose else {
        return Ok(());
    };
    if intent.kind != PrFormKind::Base || form.values().get("body") == Some(&intent.target.base_ref)
    {
        return Ok(());
    }
    let refused = |error: String| {
        let mut form = form.clone();
        form.invalidate_plan();
        form.submitting = false;
        form.error = error.clone();
        Box::new(PreparedView {
            form: Some(form),
            status: format!("PR intent refused • {error}"),
            ..Default::default()
        })
    };
    let chosen = form
        .base_selection
        .as_ref()
        .ok_or_else(|| refused("Choose a verified Base branch before Preview".into()))?;
    chosen.validate(intent, &form.values()).map_err(refused)?;
    let read = pr_metadata_catalogue::load_branch(
        provider,
        PrMetadataOriginal {
            identity: &intent.identity,
            target: &intent.target,
            context: chosen.source.context(),
        },
        chosen.name().map_err(refused)?,
        &PrMetadataReadLimits::default(),
        PrMetadataLimits::default(),
        cancel,
    );
    let fresh = match close_read(provider, intent, read, cancel) {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => return Err(refused(error)),
        Err(hidden) => return Err(hidden),
    };
    chosen
        .selected
        .validate_catalogue(fresh.prepared())
        .map_err(refused)
}
