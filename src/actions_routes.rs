//! Worker-side Actions forms. Original requests are never inferred from edited fields.
use crate::{
    actions_controller::{self, ActionsExecution},
    actions_workspace::ActionsWorkspace,
    actions_write_journal::{ActionsWriteJournal, ActionsWriteMarker, ActionsWriteState},
    drafts::DraftClient,
    forms::{Form, FormPurpose},
    providers::{gh::GithubProvider, runner::CancellationToken},
    runtime::PreparedView,
    storage::Cache,
};
use std::path::Path;

/// Closed metadata only: neither a request body nor a secret value is copied here.
pub fn original_lines(marker: &ActionsWriteMarker) -> Vec<String> {
    let state = match marker.state {
        ActionsWriteState::Pending => "Pending invocation/result • effect not confirmed",
        ActionsWriteState::Uncertain => {
            "Outcome unknown • inspect before another overlapping request"
        }
        ActionsWriteState::AcceptedPending => {
            "GitHub accepted the original request • completion not confirmed"
        }
        ActionsWriteState::Confirmed => {
            "Original synchronous effect confirmed and durably recorded"
        }
        ActionsWriteState::Acknowledged => {
            "Original inspection explicitly acknowledged • no effect was repeated"
        }
        ActionsWriteState::TransitionObserved => {
            "Later transition observed • not proof the original request caused it"
        }
    };
    let mut lines = vec![
        format!(
            "ORIGINAL ACTIONS ATTEMPT #{} • {:?}",
            marker.attempt_id, marker.kind
        ),
        state.into(),
        "This is the saved ORIGINAL target, not newly edited form fields. Nothing is resent."
            .into(),
        format!(
            "Verified account: {} / {}",
            marker.target.scope().host,
            marker.target.scope().login
        ),
        format!("Original target: {:?}", marker.target.target()),
    ];
    if let Some(run) = marker.context.run_id {
        lines.push(format!(
            "Original run: #{run} • attempt {}",
            marker
                .context
                .attempt
                .map_or_else(|| "unavailable".into(), |attempt| attempt.to_string())
        ));
    }
    if let Some(head) = &marker.context.head_sha {
        lines.push(format!("Original immutable head: {head}"));
    }
    if let Some(commit) = &marker.context.workflow_commit {
        lines.push(format!("Original workflow commit: {commit}"));
    }
    if let Some(job) = marker.context.job_id {
        lines.push(format!("Original selected job: #{job}"));
    }
    if let Some(receipt) = &marker.receipt {
        lines.push(format!(
            "Recorded result: {:?} • resource {:?} • observed attempt {:?}",
            receipt.kind, receipt.resource_id, receipt.observed_attempt
        ));
        if let Some(run) = receipt.observed_run_id {
            lines.push(format!(
                "Correlated launch: run #{run} • correlation is not workflow completion"
            ));
        }
    }
    lines.push("Inspect/acknowledge unresolved results in Actions. F9 New intent changes only this local draft; durable barriers remain.".into());
    lines
}

fn original(
    mut form: Form,
    mut workspace: ActionsWorkspace,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    let reference = form
        .actions_publication
        .as_ref()
        .ok_or("No original Actions reference")?;
    let journal = ActionsWriteJournal::open(database).map_err(|error| error.to_string())?;
    let result = actions_controller::restore_original(
        provider,
        &journal,
        &workspace.identity,
        reference,
        cancel,
    );
    form.submitting = false;
    match result {
        ActionsExecution::RestoredOriginal { marker } => {
            let warning = crate::actions_recovery::decorate_or_unavailable(
                &mut workspace,
                provider,
                &journal,
            );
            if let FormPurpose::Actions(intent) = &mut form.purpose {
                intent.workspace = workspace.clone();
            }
            form.actions_original_lines = original_lines(&marker);
            form.preview = true;
            form.preview_scroll = 0;
            form.invalidate_plan();
            Ok(PreparedView {
                context: Some(workspace.context()),
                status:format!("Original Actions attempt retained • nothing resent; inspect its recorded state{}",warning.map_or(String::new(),|warning|format!(" • {warning}"))),
                actions: Some(workspace),
                form: Some(form),
                ..Default::default()
            })
        }
        ActionsExecution::Refused {
            error,
            private_visible,
        } => {
            form.error = error.clone();
            form.invalidate_plan();
            Ok(PreparedView {
                context: private_visible.then(|| workspace.context()),
                status: format!("Original Actions attempt unavailable • {error}"),
                actions: private_visible.then_some(workspace),
                form: private_visible.then_some(form),
                clear_private: !private_visible,
                ..Default::default()
            })
        }
        _ => Err(
            "Original Actions inspection returned an invalid worker outcome; nothing may be resent"
                .into(),
        ),
    }
}

pub fn open(
    mut workspace: ActionsWorkspace,
    command: crate::actions_workspace::ActionsCommand,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    let expected = workspace.identity.clone();
    let current = provider.identity(cancel);
    let Ok(current) = current else {
        return Ok(PreparedView {
            clear_private: true,
            status: "Actions account unavailable • private drafts hidden; nothing sent".into(),
            ..Default::default()
        });
    };
    if current.account_id.is_none_or(|id| id == 0) || !current.same_account(&expected) {
        return Ok(PreparedView {
            clear_private: true,
            status: "Actions account changed • private drafts hidden; nothing sent".into(),
            ..Default::default()
        });
    }
    workspace.identity = current;
    let mut form = Form::actions(workspace.clone(), command)?;
    if let Some(draft) = Cache::open(database)
        .map_err(|error| error.to_string())?
        .draft(&form.scope(), &form.resource())
        .map_err(|error| error.to_string())?
    {
        form.restore_draft(&draft)?;
    }
    if form.actions_publication.is_some() {
        return original(form, workspace, provider, database, cancel);
    }
    if !provider.identity(cancel).is_ok_and(|identity| {
        identity.account_id.is_some_and(|id| id > 0) && identity.same_account(&workspace.identity)
    }) {
        return Ok(PreparedView {clear_private:true,status:"Actions account changed or unavailable before displaying draft • private data hidden; nothing sent".into(),..Default::default()});
    }
    let journal = ActionsWriteJournal::open(database).map_err(|error| error.to_string())?;
    let warning =
        crate::actions_recovery::decorate_or_unavailable(&mut workspace, provider, &journal);
    if let FormPurpose::Actions(intent) = &mut form.purpose {
        intent.workspace = workspace.clone();
    }
    Ok(PreparedView {
        context: Some(workspace.context()),
        status: format!(
            "Actions intent ready • nothing changed; Preview before confirmation{}",
            warning.map_or(String::new(), |warning| format!(" • {warning}"))
        ),
        actions: Some(workspace),
        form: Some(form),
        ..Default::default()
    })
}

/// Called BEFORE ActionsPlan::prepare, including for restored empty secret inputs.
pub fn preview_original(
    form: Form,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    let FormPurpose::Actions(intent) = &form.purpose else {
        return Err("Not an Actions form".into());
    };
    let workspace = intent.workspace.clone();
    original(form, workspace, provider, database, cancel)
}

pub fn submit(
    mut form: Form,
    provider: &GithubProvider,
    database: &Path,
    drafts: &DraftClient,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    let FormPurpose::Actions(intent) = &form.purpose else {
        return Err("Not an Actions form".into());
    };
    let intent = intent.as_ref().clone();
    if form.actions_publication.is_some() {
        return original(form, intent.workspace, provider, database, cancel);
    }
    let plan = intent
        .plan
        .as_ref()
        .ok_or("Choose Preview before confirmation")?;
    let mut journal = ActionsWriteJournal::open(database).map_err(|error| error.to_string())?;
    let result = actions_controller::execute(
        provider,
        &mut journal,
        plan,
        true,
        None,
        |marker| {
            form.actions_publication = Some(marker.publication_reference());
            form.actions_original_lines = original_lines(marker);
            drafts.save_and_wait(form.scope(), form.resource(), form.draft())
        },
        cancel,
    );
    present(result, form, intent.workspace, provider, &journal, drafts)
}

fn present(
    result: ActionsExecution,
    mut form: Form,
    mut workspace: ActionsWorkspace,
    provider: &GithubProvider,
    journal: &ActionsWriteJournal,
    drafts: &DraftClient,
) -> Result<PreparedView, String> {
    form.submitting = false;
    form.dirty = false;
    for field in &mut form.fields {
        if field.secret {
            field.editor.clear();
        }
    }
    let mut clear_private = false;
    let mut remove_draft = false;
    let mut status = match result {
        ActionsExecution::Refused {
            error,
            private_visible,
        } => {
            clear_private = !private_visible;
            form.error = error.clone();
            format!("Actions intent refused • {error}")
        }
        ActionsExecution::RestoredOriginal { marker } => {
            form.actions_publication = Some(marker.publication_reference());
            form.actions_original_lines = original_lines(&marker);
            "Original Actions attempt retained • nothing resent".into()
        }
        ActionsExecution::NoRemoteEffect { outcome } => {
            workspace = outcome.workspace;
            form.submitted = true;
            remove_draft = true;
            format!("Action result • {}", outcome.message)
        }
        ActionsExecution::Unknown {
            publication,
            error,
            private_visible,
        } => {
            form.actions_publication = Some(publication);
            clear_private = !private_visible;
            form.error = error.clone();
            format!("Actions outcome unknown • {error}")
        }
        ActionsExecution::Confirmed {
            publication,
            outcome,
            ..
        } => {
            form.actions_publication = Some(publication);
            workspace = outcome.workspace;
            form.submitted = true;
            remove_draft = true;
            format!("Action result • Effect confirmed • {}", outcome.message)
        }
        ActionsExecution::AcceptedPending {
            publication,
            outcome,
            ..
        } => {
            form.actions_publication = Some(publication);
            workspace = outcome.workspace;
            form.submitted = true;
            format!(
                "Action result • Request accepted; completion not confirmed • {}",
                outcome.message
            )
        }
        ActionsExecution::ReceiptPersistenceFailed {
            publication,
            error,
            private_visible,
            ..
        } => {
            form.actions_publication = Some(publication);
            clear_private = !private_visible;
            form.error = error.clone();
            format!("Actions result receipt unavailable • {error}")
        }
        ActionsExecution::EffectConfirmedButRefreshFailed {
            publication,
            outcome,
            error,
            private_visible,
            ..
        } => {
            form.actions_publication = Some(publication);
            clear_private = !private_visible;
            if let Some(outcome) = outcome {
                workspace = outcome.workspace;
            }
            form.submitted = true;
            remove_draft = true;
            format!("Action result • Effect confirmed; refresh failed • {error}")
        }
        ActionsExecution::AcceptedButRefreshFailed {
            publication,
            outcome,
            error,
            private_visible,
            ..
        } => {
            form.actions_publication = Some(publication);
            clear_private = !private_visible;
            if let Some(outcome) = outcome {
                workspace = outcome.workspace;
            }
            form.submitted = true;
            format!(
                "Action result • Request accepted; completion not confirmed; refresh failed • {error}"
            )
        }
    };
    form.invalidate_plan();
    if !clear_private {
        if let Some(reference) = &form.actions_publication {
            match journal.original_publication(provider,&workspace.identity,reference) {
                Ok(marker) => form.actions_original_lines = original_lines(&marker),
                Err(_) => form.actions_original_lines = vec!["Original receipt visibility unavailable • do not resend; inspect the saved attempt with its original account".into()],
            }
        }
        if let Some(error) =
            crate::actions_recovery::decorate_or_unavailable(&mut workspace, provider, journal)
        {
            status.push_str(&format!(" • {error}"));
        }
        if let FormPurpose::Actions(intent) = &mut form.purpose {
            intent.workspace = workspace.clone();
        }
    }
    let status = if remove_draft {
        match drafts.remove(form.scope(), form.resource()) {
            Ok(()) => status,
            Err(error) => format!(
                "{status} • draft cleanup failed: {error}; original reference retained, do not resend"
            ),
        }
    } else {
        status
    };
    Ok(PreparedView {
        context: (!clear_private).then(|| workspace.context()),
        status,
        actions: (!clear_private).then_some(workspace),
        form: (!clear_private).then_some(form),
        clear_private,
        clear_actions_inspection: true,
        ..Default::default()
    })
}

#[cfg(test)]
#[path = "actions_routes_tests.rs"]
mod tests;
